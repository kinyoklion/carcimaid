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

use crate::ir::{ArrowType, NodeShape, Palette};
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
    //    Colours come from the theme palette (default palette = the historical
    //    hardcoded values, so default-theme output is byte-unchanged).
    let palette = chart.theme.palette();
    let pal = &palette;
    s.push_str(&style_block(pal));

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
    render_scope(&mut s, &rel_nodes, &rel_edges, &rel_clusters, &chart.scope_offsets, None, (0.0, 0.0), chart.look.roughness(), pal);
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

    // 3b. Non-default themes append a `<linearGradient>` (the neo-look gradient
    //     stroke) after the defs and before the title — mermaid emits it for
    //     every theme except default, so matching it keeps themed diagrams from
    //     showing a spurious "missing element" diff.
    if let Some((stop0, stop1)) = pal.gradient_stops {
        let _ = write!(
            s,
            concat!(
                r#"<linearGradient id="{id}-gradient" gradientUnits="objectBoundingBox" "#,
                r#"x1="0%" y1="0%" x2="100%" y2="0%">"#,
                r#"<stop offset="0%" stop-color="{s0}" stop-opacity="1"/>"#,
                r#"<stop offset="100%" stop-color="{s1}" stop-opacity="1"/></linearGradient>"#,
            ),
            id = ID,
            s0 = stop0,
            s1 = stop1,
        );
    }

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
fn style_block(pal: &Palette) -> String {
    // Colours are pulled from the theme palette; the default palette reproduces
    // the historical hardcoded values, so default-theme output is byte-identical.
    // The comparator ignores <style> text, so per-theme colours here affect
    // visual fidelity but not the structural diff.
    let css = format!(
        concat!(
            "SVGID{{font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:16px;fill:{text};}}",
            "SVGID .label{{font-family:\"trebuchet ms\",verdana,arial,sans-serif;color:{text};}}",
            "SVGID .label text{{fill:{text};}}",
            "SVGID .rough-node .label text,SVGID .node .label text{{text-anchor:middle;}}",
            // Shape elements inside a classic node default to the theme node colour.
            // Icon shapes (fork/sm-circ/f-circ/…) carry a class other than
            // `.label-container` (or a presentation-attribute fill mermaid overrides),
            // so this rule is what paints them the theme node colour rather than
            // SVG-default black. Presentation attributes lose to a stylesheet rule;
            // inline `style=` (classDef colours) still wins, so styled nodes keep
            // their fill.
            "SVGID .node rect,SVGID .node circle,SVGID .node ellipse,SVGID .node polygon,SVGID .node path{{fill:{nbkg};stroke:{nborder};stroke-width:1px;}}",
            "SVGID .label-container{{fill:{nbkg};stroke:{nborder};stroke-width:1px;}}",
            "SVGID .cluster rect{{fill:{cbkg};stroke:{cborder};stroke-width:1px;}}",
            "SVGID .flowchart-link{{stroke:{line};fill:none;}}",
            "SVGID .edgeLabel{{background-color:{elbg};}}",
            "SVGID .edgeLabel rect{{opacity:0.5;fill:{elbg};}}",
            "SVGID .marker{{fill:{line};stroke:{line};}}",
            "SVGID .arrowMarkerPath{{fill:{line};stroke:{line};}}",
        ),
        text = pal.text_color,
        nbkg = pal.node_bkg,
        nborder = pal.node_border,
        cbkg = pal.cluster_bkg,
        cborder = pal.cluster_border,
        line = pal.line_color_css,
        elbg = pal.edge_label_bg,
    );
    format!("<style>{}</style>", css.replace("SVGID", &format!("#{ID}")))
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
    pal: &Palette,
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
            render_cluster(s, cluster, roughness, pal);
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
        render_node(s, node, roughness, pal);
    }
    // Nested extracted subgraphs belonging to this scope (clusters are pre-sorted
    // into mermaid's render order).
    for cluster in clusters.iter().filter(|c| c.extracted && c.home == owner) {
        render_scope(s, nodes, edges, clusters, scope_offsets, Some(cluster.sg_index), my_off, roughness, pal);
    }
    s.push_str("</g>");

    s.push_str("</g>"); // close g.root
}

fn render_cluster(s: &mut String, cluster: &PlacedCluster, roughness: f64, pal: &Palette) {
    let x = cluster.cx - cluster.width / 2.0;
    let y = cluster.cy - cluster.height / 2.0;
    let label_cls;
    if is_hand_drawn(roughness) {
        // Hand-drawn cluster: a rough rectangle (hachure fill + sketch outline)
        // through roughr, mirroring the node pattern. Theme cluster colours are
        // baked as presentation attributes; the subgraph's own `style` (fill /
        // stroke / stroke-width) rides along as an `!important` inline override,
        // exactly as mermaid emits it. The group carries no transform — the
        // rough paths are already in absolute (diagram) coordinates.
        let _ = write!(
            s,
            r#"<g class="cluster{} " id="{}-{}" data-look="handDrawn"><g>"#,
            class_suffix(&cluster.classes),
            ID,
            escape(&cluster.id),
        );
        let o = rough_options(roughness, pal);
        let drawable = roughr::Generator::new().rectangle(x, y, cluster.width, cluster.height, &o);
        // Fill hachure: theme clusterBkg, with the subgraph `fill` (if any)
        // overriding via `stroke:` (a hachure fill paints as a stroke). mermaid
        // draws these lines at a fixed weight of 3.
        let fill_style = match css_value(&cluster.shape_style, "fill") {
            Some(f) => format!(r#"stroke:{} !important"#, f),
            None => String::new(),
        };
        let _ = write!(
            s,
            r#"<path d="{}" stroke="{}" stroke-width="3" fill="none" stroke-dasharray="0 0" style="{}"/>"#,
            drawable.fill_path(None),
            pal.cluster_bkg,
            escape(&fill_style),
        );
        // Outline: theme clusterBorder, width from the subgraph `stroke-width`
        // (default 1.3); the subgraph `style` minus its `fill` rides along.
        let sw = css_value(&cluster.shape_style, "stroke-width")
            .map(|w| w.trim_end_matches("px").trim().to_string())
            .unwrap_or_else(|| "1.3".to_string());
        let _ = write!(
            s,
            r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0" style="{}"/>"#,
            drawable.stroke_path(None),
            pal.cluster_border,
            sw,
            escape(&style_without_fill(&cluster.shape_style)),
        );
        s.push_str("</g>");
        label_cls = "cluster-label ";
    } else {
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
        label_cls = "cluster-label";
    }
    // The label sits centred at the top of the cluster box.
    let label_x = cluster.cx - crate::text::measure_width(&cluster.title, 16.0) / 2.0;
    let _ = write!(
        s,
        r#"<g class="{}" transform="translate({}, {})"><g>"#,
        label_cls,
        round(label_x),
        round(y),
    );
    s.push_str(r#"<rect class="background" style="stroke: none"/>"#);
    render_text(s, Some(&cluster.title), false, "");
    s.push_str("</g></g></g>");
}

/// A `;`-separated style declaration string with any `fill:` declaration
/// dropped (used for a hand-drawn cluster's outline, whose fill rides on the
/// separate hachure path instead).
fn style_without_fill(style: &str) -> String {
    style
        .split(';')
        .filter(|decl| !decl.trim().is_empty())
        .filter(|decl| decl.split_once(':').map(|(k, _)| k.trim() != "fill").unwrap_or(true))
        .collect::<Vec<_>>()
        .join(";")
}

fn render_node(s: &mut String, node: &PlacedNode, roughness: f64, pal: &Palette) {
    // The hand-drawn look uses mermaid's rough-node group (`rough-node default`,
    // `data-look="handDrawn"`); the classic look uses the flat `node default`
    // group. mermaid emits a trailing space after `default` for rough nodes.
    if roughness > 0.0 {
        let _ = write!(
            s,
            r#"<g class="rough-node default {} " id="{}-flowchart-{}-0" data-look="handDrawn" transform="translate({}, {})">"#,
            node.classes.join(" "),
            ID,
            escape(&node.id),
            round(node.cx),
            round(node.cy),
        );
    } else {
        let _ = write!(
            s,
            r#"<g class="node default{}" id="{}-flowchart-{}-0" data-look="classic" transform="translate({}, {})">"#,
            class_suffix(&node.classes),
            ID,
            escape(&node.id),
            round(node.cx),
            round(node.cy),
        );
    }
    render_shape(s, node, roughness, pal);
    // Icon shapes (fork/join, the state start/stop/junction circles, the crossed
    // circle, hourglass, lightning bolt) are fixed-size glyphs that mermaid draws
    // with *no* label: their handlers set `node.label = ""` and never call
    // `labelHelper`, so the node group has no `g.label` child at all. Emit the
    // shape and stop, matching the oracle DOM.
    if is_labelless(node.shape) {
        s.push_str("</g>");
        return;
    }
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
fn render_shape(s: &mut String, node: &PlacedNode, roughness: f64, pal: &Palette) {
    let (hw, hh) = (node.width / 2.0, node.height / 2.0);
    let st = style_attr(&node.shape_style);
    // Per-node hand-drawn colours (classDef/`style` fill/stroke, or theme
    // defaults). Baked into the rough paths for the hand-drawn look.
    let (hd_fill, hd_stroke, hd_sw) = hand_drawn_colors(&node.shape_style, pal);
    match node.shape {
        NodeShape::Rectangle | NodeShape::DataStore if is_hand_drawn(roughness) => {
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().rectangle(-hw, -hh, node.width, node.height, &o);
            let _ = write!(s, r#"<g class="basic label-container"{}>"#, hd_style(&node.shape_style));
            emit_hd_drawable(s, &drawable, false, &hd_fill, &hd_stroke, &hd_sw);
            s.push_str("</g>");
        }
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
        NodeShape::RoundedRectangle | NodeShape::Stadium if is_hand_drawn(roughness) => {
            let is_stadium = matches!(node.shape, NodeShape::Stadium);
            let r = if is_stadium { hh } else { 5.0 };
            let d = rounded_rect_path(-hw, -hh, node.width, node.height, r);
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&d, &o);
            // The stadium (pill) carries mermaid's `outer-path` class and no
            // `style` attr; the rounded rectangle carries `style` and no
            // `outer-path` — matching the oracle DOM.
            if is_stadium {
                s.push_str(r#"<g class="basic label-container outer-path">"#);
            } else {
                let _ = write!(s, r#"<g class="basic label-container"{}>"#, hd_style(&node.shape_style));
            }
            emit_hd_drawable(s, &drawable, false, &hd_fill, &hd_stroke, &hd_sw);
            s.push_str("</g>");
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
        NodeShape::Circle if is_hand_drawn(roughness) => {
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().circle(0.0, 0.0, node.width, &o);
            let _ = write!(s, r#"<g class="basic label-container"{}>"#, hd_style(&node.shape_style));
            emit_hd_drawable(s, &drawable, false, &hd_fill, &hd_stroke, &hd_sw);
            s.push_str("</g>");
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
        NodeShape::Rhombus if is_hand_drawn(roughness) => {
            let side = node.width;
            let pts = [
                [side / 2.0, 0.0],
                [side, -side / 2.0],
                [side / 2.0, -side],
                [0.0, -side / 2.0],
            ];
            emit_hd_polygon(s, &pts, -side / 2.0 + 0.5, side / 2.0, &node.shape_style, roughness, pal);
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
        NodeShape::Hexagon if is_hand_drawn(roughness) => {
            let (w, h) = (node.width, node.height);
            let m = h / 4.0;
            let pts = [
                [m, 0.0],
                [w - m, 0.0],
                [w, -h / 2.0],
                [w - m, -h],
                [m, -h],
                [0.0, -h / 2.0],
            ];
            emit_hd_polygon(s, &pts, -w / 2.0, h / 2.0, &node.shape_style, roughness, pal);
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
        NodeShape::Subroutine if is_hand_drawn(roughness) => {
            // Outer rectangle plus two inner vertical bars (inset 8px), each a
            // separate rough drawable — matching mermaid's handDrawn subroutine
            // (1 hachure fill + 3 strokes).
            let (w, h) = (node.width, node.height);
            let inset = hw - 8.0;
            let gen = roughr::Generator::new();
            let o = rough_options(roughness, pal);
            let rect = gen.rectangle(-hw, -hh, w, h, &o);
            let left = gen.line(-inset, -hh, -inset, hh, &o);
            let right = gen.line(inset, -hh, inset, hh, &o);
            let _ = write!(s, r#"<g class="basic label-container"{}>"#, hd_style(&node.shape_style));
            emit_hd_drawable(s, &rect, false, &hd_fill, &hd_stroke, &hd_sw);
            emit_drawable(s, &left, false, "", None, &hd_stroke, &hd_sw);
            emit_drawable(s, &right, false, "", None, &hd_stroke, &hd_sw);
            s.push_str("</g>");
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
        NodeShape::Parallelogram if is_hand_drawn(roughness) => {
            let (w, h) = (node.width - node.height, node.height);
            let pts = [[-h / 2.0, 0.0], [w, 0.0], [w + h / 2.0, -h], [0.0, -h]];
            emit_hd_polygon(s, &pts, -w / 2.0, h / 2.0, &node.shape_style, roughness, pal);
        }
        NodeShape::Parallelogram => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(-h / 2.0, 0.0), (w, 0.0), (w + h / 2.0, -h), (0.0, -h)], -w / 2.0, h / 2.0, &st);
        }
        NodeShape::LeanLeft if is_hand_drawn(roughness) => {
            let (w, h) = (node.width - node.height, node.height);
            let pts = [[0.0, 0.0], [w + h / 2.0, 0.0], [w, -h], [-h / 2.0, -h]];
            emit_hd_polygon(s, &pts, -w / 2.0, h / 2.0, &node.shape_style, roughness, pal);
        }
        NodeShape::LeanLeft => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(0.0, 0.0), (w + h / 2.0, 0.0), (w, -h), (-h / 2.0, -h)], -w / 2.0, h / 2.0, &st);
        }
        NodeShape::Trapezoid if is_hand_drawn(roughness) => {
            let (w, h) = (node.width - node.height, node.height);
            let pts = [[-h / 2.0, 0.0], [w + h / 2.0, 0.0], [w, -h], [0.0, -h]];
            emit_hd_polygon(s, &pts, -w / 2.0, h / 2.0, &node.shape_style, roughness, pal);
        }
        NodeShape::Trapezoid => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(-h / 2.0, 0.0), (w + h / 2.0, 0.0), (w, -h), (0.0, -h)], -w / 2.0, h / 2.0, &st);
        }
        NodeShape::InvTrapezoid if is_hand_drawn(roughness) => {
            let (w, h) = (node.width - node.height, node.height);
            let pts = [[0.0, 0.0], [w, 0.0], [w + h / 2.0, -h], [-h / 2.0, -h]];
            emit_hd_polygon(s, &pts, -w / 2.0, h / 2.0, &node.shape_style, roughness, pal);
        }
        NodeShape::InvTrapezoid => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(0.0, 0.0), (w, 0.0), (w + h / 2.0, -h), (-h / 2.0, -h)], -w / 2.0, h / 2.0, &st);
        }
        NodeShape::Cylinder if is_hand_drawn(roughness) => {
            let w = node.width;
            let ry = crate::layout::cylinder_ry(w);
            let rx = w / 2.0;
            let body_h = node.height - 2.0 * ry;
            // Feed roughr the *silhouette* (a single closed loop: top back arc,
            // sides, bottom front arc) so the hachure fills the whole body —
            // including up under the top ellipse — as continuous diagonals.
            // Passing the full outline (with the doubled top ellipse) instead
            // makes roughr treat the cap as its own tiny sub-loop and cram it
            // with a dense black band of short hachure segments.
            let d = cylinder_silhouette(ry, rx, w, body_h);
            // The top ellipse's front (visible opening) arc is drawn as a
            // separate stroke in its own `<g>`, in node-centred coords, matching
            // mermaid's handDrawn cylinder.
            let rim_d = format!(
                "M{sx},{sy} a{rx},{ry} 0,0,0 {w},0",
                sx = round(-w / 2.0),
                sy = round(-node.height / 2.0 + ry),
                rx = round(rx),
                ry = round(ry),
                w = round(w),
            );
            let gen = roughr::Generator::new();
            let o = rough_options(roughness, pal);
            let body = gen.path(&d, &o);
            let rim = gen.path(&rim_d, &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container"{} label-offset-y="{}" transform="translate({}, {})">"#,
                hd_style(&node.shape_style),
                round(ry),
                round(-w / 2.0),
                round(-node.height / 2.0),
            );
            emit_hd_drawable(s, &body, false, &hd_fill, &hd_stroke, &hd_sw);
            s.push_str("</g><g>");
            emit_drawable(s, &rim, false, "", None, &hd_stroke, &hd_sw);
            s.push_str("</g>");
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
        // Hand-drawn start circle (stateStart): a small rough circle (r=7)
        // rendered like any other node — node-fill hachure + border stroke.
        // (mermaid's markup carries `fill="none"`/black paths, but its forest
        // theme CSS overrides them with the node fill/border via `!important`;
        // we bake the theme colours straight into the paths.) Without this the
        // classic branch's `<circle fill:#000000>` draws a solid black disc.
        NodeShape::SmallCircle if is_hand_drawn(roughness) => {
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().circle(0.0, 0.0, 14.0, &o);
            s.push_str(r#"<g class="state-start" r="7" width="14" height="14">"#);
            emit_hd_drawable(s, &drawable, false, &hd_fill, &hd_stroke, &hd_sw);
            s.push_str("</g>");
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
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().polygon(&pts, &o);
            let _ = write!(s, r#"<g class="basic label-container outer-path">"#);
            emit_rough_drawable(s, &drawable, true, &st, pal);
            s.push_str("</g>");
        }
        // Lined/shaded process: mermaid `rc.polygon` — a rectangle plus a left
        // bar (the polygon doubles back along the divider). rc.polygon sets
        // `fill-rule="evenodd"`.
        NodeShape::LinedProcess => {
            let frame = 8.0;
            let total_w = node.width;
            let h = node.height;
            let w = total_w - frame;
            let x = frame - total_w / 2.0;
            let y = -h / 2.0;
            let pts = [
                [x, y],
                [x + w, y],
                [x + w, y + h],
                [x - frame, y + h],
                [x - frame, y],
                [x, y],
                [x, y + h],
            ];
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().polygon(&pts, &o);
            s.push_str(r#"<g class="basic label-container outer-path">"#);
            emit_rough_drawable(s, &drawable, true, &st, pal);
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
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_d, &o);
            let _ = write!(s, r#"<g transform="translate(5, 5)" class="basic label-container outer-path">"#);
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
        }
        // Stacked rectangle / multi-process: mermaid draws an outer and an inner
        // `rc.path`, each collapsed to one element via `mergePaths` for the
        // classic look. The dagre bbox overflows the body by `rectOffset` (5) on
        // each side, so recover `w = node.width - 10`, `h = node.height - 10`.
        NodeShape::StackedRect => {
            let ro = 5.0;
            let w = node.width - 2.0 * ro;
            let h = node.height - 2.0 * ro;
            let x = -w / 2.0;
            let y = -h / 2.0;
            let outer = [
                [x - ro, y + ro],
                [x - ro, y + h + ro],
                [x + w - ro, y + h + ro],
                [x + w - ro, y + h],
                [x + w, y + h],
                [x + w, y + h - ro],
                [x + w + ro, y + h - ro],
                [x + w + ro, y - ro],
                [x + ro, y - ro],
                [x + ro, y],
                [x, y],
                [x, y + ro],
            ];
            let inner = [
                [x, y + ro],
                [x + w - ro, y + ro],
                [x + w - ro, y + h],
                [x + w, y + h],
                [x + w, y],
                [x, y],
            ];
            let o = rough_options(roughness, pal);
            let outer_d = roughr::Generator::new().path(&path_from_points(&outer), &o);
            let inner_d = roughr::Generator::new().path(&path_from_points(&inner), &o);
            s.push_str(r#"<g class="basic label-container outer-path">"#);
            if is_hand_drawn(roughness) {
                // Hand-drawn look: each stacked rect is a separate rough
                // drawable in its own `<g>`, emitting a hachure fill path + a
                // sketch outline path — like every other hand-drawn shape, not
                // the classic `mergePaths` single-path-per-rect form.
                s.push_str("<g>");
                emit_hd_drawable(s, &outer_d, false, &hd_fill, &hd_stroke, &hd_sw);
                s.push_str("</g><g>");
                emit_hd_drawable(s, &inner_d, false, &hd_fill, &hd_stroke, &hd_sw);
                s.push_str("</g>");
            } else {
                emit_merged(s, &outer_d, &st, pal);
                emit_merged(s, &inner_d, &st, pal);
            }
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
        // Notched pentagon (loop limit): mermaid `rc.path` through 6 points.
        NodeShape::NotchedPentagon => {
            let (w, h) = (node.width / 2.0, node.height / 2.0);
            let pts = [
                [-w * 0.8, -h],
                [w * 0.8, -h],
                [w, -h * 0.6],
                [w, h],
                [-w, h],
                [-w, -h * 0.6],
            ];
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            s.push_str(r#"<g class="basic label-container outer-path">"#);
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
        }
        // Triangle / flipped triangle: mermaid renders these via rough.js
        // `rc.path` (base `tw` == height); the group is translated to centre it.
        NodeShape::Triangle | NodeShape::FlippedTriangle => {
            let h = node.height;
            let pts: [[f64; 2]; 3] = if matches!(node.shape, NodeShape::Triangle) {
                [[0.0, 0.0], [h, 0.0], [h / 2.0, -h]]
            } else {
                [[0.0, -h], [h, -h], [h / 2.0, 0.0]]
            };
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            let _ = write!(
                s,
                r#"<g transform="translate({}, {})" class="outer-path">"#,
                round(-h / 2.0),
                round(h / 2.0),
            );
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
        }
        // Sloped rectangle (mermaid `rc.path`). The drawn height spans 1.5·h
        // (node.height); the shape body uses h = node.height/1.5. Note mermaid's
        // class has a double space ("basic label-container  outer-path").
        NodeShape::SlopedRect => {
            let w = node.width;
            let h = node.height / 1.5;
            let (x, y) = (-w / 2.0, -h / 2.0);
            let pts = [[x, y], [x, y + h], [x + w, y + h], [x + w, y - h / 2.0]];
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container  outer-path" transform="translate(0, {})">"#,
                round(h / 4.0),
            );
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
        }
        // Curved trapezoid (display): mermaid `rc.path` — trapezoid with an arced
        // left edge (a sampled half-ellipse). Points in mermaid's [0,w]×[0,h]
        // frame, then centred via `translate(-w/2, -h/2)`.
        NodeShape::CurvedTrapezoid => {
            let (w, h) = (node.width, node.height);
            let radius = h / 2.0;
            let rw = w - radius;
            let tw = h / 4.0;
            let mut pts: Vec<[f64; 2]> = vec![
                [rw, 0.0],
                [tw, 0.0],
                [0.0, h / 2.0],
                [tw, h],
                [rw, h],
            ];
            pts.extend(generate_circle_points(-rw, -h / 2.0, radius, 50, 270.0, 90.0));
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate({}, {})">"#,
                round(-w / 2.0),
                round(-h / 2.0),
            );
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
        }
        // Filled junction circle (filledCircle, r=7, no label): mermaid
        // `rc.circle` (solid), both paths styled `fill: nodeBorder !important`.
        NodeShape::FilledCircle if is_hand_drawn(roughness) => {
            // The hand-drawn filled disc is a SOLID rough circle (node fill +
            // outline), not a hachure sketch — mermaid does *not* apply the
            // classic look's `fill: nodeBorder !important` override here. Force
            // the solid fill style so a tiny disc reads as a filled dot, not a
            // black hachure smudge.
            let mut o = rough_options(roughness, pal);
            o.fill_style = "solid".to_string();
            let drawable = roughr::Generator::new().circle(0.0, 0.0, node.width, &o);
            s.push_str("<g>");
            emit_hd_drawable(s, &drawable, false, &hd_fill, &hd_stroke, &hd_sw);
            s.push_str("</g>");
        }
        NodeShape::FilledCircle => {
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().circle(0.0, 0.0, node.width, &o);
            s.push_str("<g>");
            let fc_style = format!(r#" style="fill: {} !important;""#, pal.node_border);
            emit_drawable(s, &drawable, false, &fc_style, Some(pal.node_bkg), pal.node_border, "1.3");
            s.push_str("</g>");
        }
        // Framed stop circle (stateEnd): an outer `rc.circle` (stroke = line
        // colour, width 2) and a filled inner `rc.circle` (fill/stroke =
        // node border). Inner is nested in its own `<g>`.
        NodeShape::FramedCircle => {
            let o = rough_options(roughness, pal);
            let gen = roughr::Generator::new();
            let mut outer_o = o.clone();
            outer_o.stroke = pal.line_color.to_string();
            let outer = gen.circle(0.0, 0.0, node.width, &outer_o);
            let mut inner_o = o.clone();
            inner_o.fill = Some(pal.node_border.to_string());
            inner_o.stroke = pal.node_border.to_string();
            // The inner disc is a SOLID fill (node border), not a hachure
            // sketch, in both the classic and hand-drawn looks.
            inner_o.fill_style = "solid".to_string();
            let inner = gen.circle(0.0, 0.0, node.width * 5.0 / 14.0, &inner_o);
            s.push_str(r#"<g class="outer-path">"#);
            emit_drawable(s, &outer, false, &st, Some(pal.node_bkg), pal.line_color, "2");
            s.push_str("<g>");
            emit_drawable(s, &inner, false, &st, Some(pal.node_border), pal.node_border, "2");
            s.push_str("</g></g>");
        }
        // Crossed circle (crossedCircle, r=30, no label): a `rc.circle` with an X
        // (`rc.path`) drawn across it; the line's fill is empty. Inner line is
        // nested in its own `<g>`.
        NodeShape::CrossedCircle => {
            let r = node.width / 2.0;
            let o = rough_options(roughness, pal);
            let gen = roughr::Generator::new();
            let circle = gen.circle(0.0, 0.0, node.width, &o);
            // mermaid's createLine: two diagonals through the ±45° points.
            let a = r * (0.5_f64).sqrt();
            let line_d = format!("M {},{} L {},{}\n                   M {},{} L {},{}", -a, a, a, -a, a, a, -a, -a);
            let line = gen.path(&line_d, &o);
            s.push_str(r#"<g class="outer-path">"#);
            emit_rough_drawable(s, &circle, false, &st, pal);
            s.push_str("<g>");
            emit_rough_drawable(s, &line, false, &st, pal);
            s.push_str("</g></g>");
        }
        // Odd (rect_left_inv_arrow): rectangle with a notched left edge, mermaid
        // `rc.path`. The dagre bbox is `w + h/4` (the notch overflows left by
        // h/4), so recover the inner width `w = node.width - h/4`; the group is
        // shifted right by `-notch/2 = h/8`.
        NodeShape::Odd => {
            let h = node.height;
            let w = node.width - h / 4.0;
            let (x, y) = (-w / 2.0, -h / 2.0);
            let notch = y / 2.0;
            let pts = [
                [x + notch, y],
                [x, 0.0],
                [x + notch, -y],
                [-x, -y],
                [-x, y],
            ];
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate({},0)">"#,
                round(-notch / 2.0),
            );
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
        }
        // Delay (halfRoundedRectangle): rectangle with a rounded right end, drawn
        // by mermaid via `rc.path` — the right edge is a sampled semicircle.
        NodeShape::Delay => {
            let (w, h) = (node.width, node.height);
            let radius = h / 2.0;
            let mut pts: Vec<[f64; 2]> = vec![[-w / 2.0, -h / 2.0], [w / 2.0 - radius, -h / 2.0]];
            pts.extend(generate_circle_points(-w / 2.0 + radius, 0.0, radius, 50, 90.0, 270.0));
            pts.push([w / 2.0 - radius, h / 2.0]);
            pts.push([-w / 2.0, h / 2.0]);
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            s.push_str(r#"<g class="basic label-container outer-path">"#);
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
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
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_d, &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate(0,{})">"#,
                round(-wave_amp / 2.0),
            );
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
        }
        // Lined document (linedWaveEdgedRect): rectangle with a wavy bottom edge
        // plus a left divider line, all traced by a single mermaid `rc.polygon`
        // (the polygon doubles back up the divider). The dagre bbox is `1.1·w`
        // wide (the outline overflows ±w·0.05) and the divider point reaches
        // `finalH/2·1.1`; recover w/finalH from that. `fill-rule="evenodd"`.
        NodeShape::LinedDocument => {
            let w = node.width / 1.1;
            let final_h = node.height / 1.1055;
            let wave_amp = final_h / 9.0; // finalH = h + h/8 = 1.125h -> waveAmp = finalH/9
            let ext = w / 2.0 * 0.1;
            let mut pts: Vec<[f64; 2]> = vec![
                [-w / 2.0 - ext, -final_h / 2.0],
                [-w / 2.0 - ext, final_h / 2.0],
            ];
            pts.extend(full_sine_wave_points(-w / 2.0 - ext, final_h / 2.0, w / 2.0 + ext, final_h / 2.0, wave_amp, 0.8));
            pts.push([w / 2.0 + ext, -final_h / 2.0]);
            pts.push([-w / 2.0 - ext, -final_h / 2.0]);
            pts.push([-w / 2.0, -final_h / 2.0]);
            pts.push([-w / 2.0, final_h / 2.0 * 1.1]);
            pts.push([-w / 2.0, -final_h / 2.0]);
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().polygon(&pts, &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate(0,{})">"#,
                round(-wave_amp / 2.0),
            );
            emit_rough_drawable(s, &drawable, true, &st, pal);
            s.push_str("</g>");
        }
        // Tagged document (taggedWaveEdgedRectangle): a wave-edged document
        // (rc.path, nested `<g>`) plus a folded-corner tag (rc.path, drawn after).
        NodeShape::TaggedDocument => {
            let w = node.width / 1.1;
            let h = node.height / 1.25;
            let wave_amp = h / 8.0;
            let tag_w = 0.2 * w;
            let tag_h = 0.2 * h;
            let final_h = h + wave_amp;
            let ext = w / 2.0 * 0.1;
            // Document outline (rc.path).
            let mut doc: Vec<[f64; 2]> = vec![[-w / 2.0 - ext, final_h / 2.0]];
            doc.extend(full_sine_wave_points(-w / 2.0 - ext, final_h / 2.0, w / 2.0 + ext, final_h / 2.0, wave_amp, 0.8));
            doc.push([w / 2.0 + ext, -final_h / 2.0]);
            doc.push([-w / 2.0 - ext, -final_h / 2.0]);
            // Tag (rc.path).
            let x = -w / 2.0 + ext;
            let y = -final_h / 2.0 - tag_h * 0.4;
            let mut tag: Vec<[f64; 2]> = vec![
                [x + w - tag_w, (y + h) * 1.3],
                [x + w, y + h - tag_h],
                [x + w, (y + h) * 0.9],
            ];
            tag.extend(full_sine_wave_points(x + w, (y + h) * 1.25, x + w - tag_w, (y + h) * 1.3, -h * 0.02, 0.5));
            let o = rough_options(roughness, pal);
            let doc_d = roughr::Generator::new().path(&path_from_points(&doc), &o);
            let tag_d = roughr::Generator::new().path(&path_from_points(&tag), &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate(0,{})">"#,
                round(-wave_amp / 2.0),
            );
            s.push_str("<g>");
            emit_rough_drawable(s, &doc_d, false, &st, pal);
            s.push_str("</g>");
            emit_rough_drawable(s, &tag_d, false, &st, pal);
            s.push_str("</g>");
        }
        // Stacked documents (multiWaveEdgedRectangle): an outer wave-edged
        // "stack" outline plus an inner document, both mermaid `rc.path`. The
        // dagre bbox is `w + 20` wide and `19h/16 + 20` tall (the outline
        // overflows by rectOffset=10 and the wave dips waveAmp below); recover
        // w/h from that. Inner document is nested in its own `<g>`.
        NodeShape::Documents => {
            let ro = 10.0;
            let w = node.width - 20.0;
            let h = (node.height - 20.0) * 16.0 / 19.0;
            let wave_amp = h / 8.0;
            let final_h = h + wave_amp / 2.0;
            let x = -w / 2.0;
            let y = -final_h / 2.0;
            let wave = full_sine_wave_points(x - ro, y + final_h + ro, x + w - ro, y + final_h + ro, wave_amp, 0.8);
            let last = *wave.last().unwrap();
            let mut outer: Vec<[f64; 2]> = vec![[x - ro, y + ro], [x - ro, y + final_h + ro]];
            outer.extend_from_slice(&wave);
            outer.extend_from_slice(&[
                [x + w - ro, last[1] - ro],
                [x + w, last[1] - ro],
                [x + w, last[1] - 2.0 * ro],
                [x + w + ro, last[1] - 2.0 * ro],
                [x + w + ro, y - ro],
                [x + ro, y - ro],
                [x + ro, y],
                [x, y],
                [x, y + ro],
            ]);
            let inner = [
                [x, y + ro],
                [x + w - ro, y + ro],
                [x + w - ro, last[1] - ro],
                [x + w, last[1] - ro],
                [x + w, y],
                [x, y],
            ];
            let o = rough_options(roughness, pal);
            let outer_d = roughr::Generator::new().path(&path_from_points(&outer), &o);
            let inner_d = roughr::Generator::new().path(&path_from_points(&inner), &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate(0,{})">"#,
                round(-wave_amp / 2.0),
            );
            emit_rough_drawable(s, &outer_d, false, &st, pal);
            s.push_str("<g>");
            emit_rough_drawable(s, &inner_d, false, &st, pal);
            s.push_str("</g></g>");
        }
        // Tagged rectangle (taggedRect): a rectangle (rc.path, nested `<g>`) plus
        // a folded-corner tag (rc.path, drawn after). The dagre bbox is
        // `w + tagWidth` wide where tagWidth = 0.2·h; recover `w`.
        NodeShape::TaggedRect => {
            let h = node.height;
            let tag = 0.2 * h; // TAG_RATIO * totalHeight
            let w = node.width - tag;
            let (x, y) = (-w / 2.0, -h / 2.0);
            let rect_pts = [
                [x - tag / 2.0, y],
                [x + w + tag / 2.0, y],
                [x + w + tag / 2.0, y + h],
                [x - tag / 2.0, y + h],
            ];
            let tag_pts = [
                [x + w - tag / 2.0, y + h],
                [x + w + tag / 2.0, y + h],
                [x + w + tag / 2.0, y + h - tag],
            ];
            let o = rough_options(roughness, pal);
            let rect_d = roughr::Generator::new().path(&path_from_points(&rect_pts), &o);
            let tag_d = roughr::Generator::new().path(&path_from_points(&tag_pts), &o);
            s.push_str(r#"<g class="basic label-container outer-path">"#);
            s.push_str("<g>");
            emit_rough_drawable(s, &rect_d, false, &st, pal);
            s.push_str("</g>");
            emit_rough_drawable(s, &tag_d, false, &st, pal);
            s.push_str("</g>");
        }
        // Bow-tie rectangle (bowTieRect): rectangle with concave (inward-arced)
        // left/right sides sampled by mermaid via `generateArcPoints`, drawn with
        // `rc.path`. `ry = h/2`, `rx = ry/(2.5 + h/50)`; the group is shifted
        // right by `rx/2`.
        NodeShape::BowTieRect => {
            let h = node.height;
            let ry = h / 2.0;
            let rx = ry / (2.5 + h / 50.0);
            // The dagre box width is `w + sagitta` (the arced sides sit inside w
            // but mermaid pads the box by the sagitta); recover the body width.
            let w = node.width - arc_sagitta(h, rx, ry);
            let mut pts: Vec<[f64; 2]> = vec![[w / 2.0, -h / 2.0], [-w / 2.0, -h / 2.0]];
            pts.extend(generate_arc_points(-w / 2.0, -h / 2.0, -w / 2.0, h / 2.0, rx, ry, false));
            pts.push([w / 2.0, h / 2.0]);
            pts.extend(generate_arc_points(w / 2.0, h / 2.0, w / 2.0, -h / 2.0, rx, ry, true));
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate({}, 0)">"#,
                round(rx / 2.0),
            );
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
        }
        // Wave rectangle (flag / paper tape): wavy top and bottom edges, mermaid
        // `rc.path`. The dagre bbox height is the drawn `finalH = 1.25·h`, so
        // recover `waveAmplitude = finalH/10` (= h/8). Class is just
        // "basic label-container" (no outer-path).
        NodeShape::WaveRect => {
            let w = node.width;
            let final_h = node.height;
            let wave_amp = final_h / 10.0;
            let mut pts: Vec<[f64; 2]> = vec![[-w / 2.0, final_h / 2.0]];
            pts.extend(full_sine_wave_points(-w / 2.0, final_h / 2.0, w / 2.0, final_h / 2.0, wave_amp, 1.0));
            pts.push([w / 2.0, -final_h / 2.0]);
            pts.extend(full_sine_wave_points(w / 2.0, -final_h / 2.0, -w / 2.0, -final_h / 2.0, wave_amp, -1.0));
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            s.push_str(r#"<g class="basic label-container">"#);
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
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
        NodeShape::LinedCylinder if is_hand_drawn(roughness) => {
            let w = node.width;
            let ry = crate::layout::cylinder_ry(w);
            let rx = w / 2.0;
            let body_h = node.height - 2.0 * ry;
            // Cylinder silhouette (single closed loop) so the hachure fill is a
            // continuous set of diagonals rather than a doubled top ellipse.
            let d = cylinder_silhouette(ry, rx, w, body_h);
            // The "lined" horizontal rim sits `2*ry` below the top, drawn (like
            // mermaid) in a separate `<g class="line">` in node-centred coords.
            let line_d = format!(
                "M{sx},{sy} a{rx},{ry} 0,0,0 {w},0",
                sx = round(-w / 2.0),
                sy = round(-node.height / 2.0 + 2.0 * ry),
                rx = round(rx),
                ry = round(ry),
                w = round(w),
            );
            let gen = roughr::Generator::new();
            let o = rough_options(roughness, pal);
            let body = gen.path(&d, &o);
            let line = gen.path(&line_d, &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container"{} label-offset-y="{}" transform="translate({}, {})">"#,
                hd_style(&node.shape_style),
                round(ry),
                round(-w / 2.0),
                round(-node.height / 2.0),
            );
            emit_hd_drawable(s, &body, false, &hd_fill, &hd_stroke, &hd_sw);
            s.push_str(r#"</g><g class="line">"#);
            emit_hd_drawable(s, &line, false, &hd_fill, &hd_stroke, &hd_sw);
            s.push_str("</g>");
        }
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
        // Fork/join: a thin solid bar (no label), mermaid `rc.rectangle` filled
        // and stroked in the theme line colour. The shape `<g>` carries no class
        // in the classic look.
        NodeShape::Fork => {
            let (w, h) = (node.width, node.height);
            let mut o = rough_options(roughness, pal);
            o.fill = Some(pal.line_color.to_string());
            o.stroke = pal.line_color.to_string();
            let drawable = roughr::Generator::new().rectangle(-w / 2.0, -h / 2.0, w, h, &o);
            s.push_str("<g>");
            emit_drawable(s, &drawable, false, &st, Some(pal.line_color), pal.line_color, "1.3");
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
        // Hourglass (collate): two triangles meeting at the centre, drawn by
        // mermaid via `rc.path` on points [(0,0),(w,0),(0,h),(w,h)] with a
        // centring `translate(-w/2, -h/2)`.
        NodeShape::Hourglass => {
            let (w, h) = (node.width, node.height);
            let pts = [[0.0, 0.0], [w, 0.0], [0.0, h], [w, h]];
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate({}, {})">"#,
                round(-w / 2.0),
                round(-h / 2.0),
            );
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
        }
        // Lightning bolt: a zig-zag drawn by mermaid via `rc.path`. The dagre
        // bbox height is `2*height`, so recover `height = node.height / 2`; the
        // group is translated by `(-width/2, -height)`.
        NodeShape::LightningBolt => {
            let width = node.width;
            let height = node.height / 2.0;
            let gap = 7.0;
            let pts = [
                [width, 0.0],
                [0.0, height + gap / 2.0],
                [width - 2.0 * gap, height + gap / 2.0],
                [0.0, 2.0 * height],
                [width, height - gap / 2.0],
                [2.0 * gap, height - gap / 2.0],
            ];
            let o = rough_options(roughness, pal);
            let drawable = roughr::Generator::new().path(&path_from_points(&pts), &o);
            let _ = write!(
                s,
                r#"<g class="outer-path" transform="translate({},{})">"#,
                round(-width / 2.0),
                round(-height),
            );
            emit_rough_drawable(s, &drawable, false, &st, pal);
            s.push_str("</g>");
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
        // Curly braces (curlyBraceLeft / Right / curlyBraces): stroke-only brace
        // outline(s) plus an invisible (stroke-opacity 0) rectangle hit-area,
        // both mermaid `rc.path` with `fill: none`. The dagre bbox adds the brace
        // arc extent to the body (w = node.width - 10 for one brace, - 12.5 for
        // both; h = node.height - 10). Wrapper `<g class="text">`.
        NodeShape::BraceLeft | NodeShape::BraceRight | NodeShape::Braces => {
            // Both-sided braces get 2.5px more dagre width than a single brace, so
            // trim an extra 2.5 to recover the body width.
            let w = node.width - if matches!(node.shape, NodeShape::Braces) { 12.5 } else { 10.0 };
            let h = node.height - 10.0;
            let r = (h * 0.1_f64).max(5.0);
            let o = rough_options(roughness, pal);
            let gen = roughr::Generator::new();
            // A fill:none `rc.path` (only its stroke `<path>` is emitted).
            let mut none_o = o.clone();
            none_o.fill = None;
            // stroke-only emitter wrapped in a `<g>` with optional attrs.
            let emit_brace = |s: &mut String, pts: &[[f64; 2]], strip_z: bool, gattr: &str| {
                let mut d = path_from_points(pts);
                if strip_z {
                    d = d.replace('Z', "");
                }
                let drawable = gen.path(&d, &none_o);
                let _ = write!(s, "<g{gattr}>");
                emit_drawable(s, &drawable, false, &st, None, pal.node_border, "1.3");
                s.push_str("</g>");
            };
            match node.shape {
                NodeShape::BraceLeft => {
                    let mut pts: Vec<[f64; 2]> = Vec::new();
                    pts.extend(generate_circle_points(w / 2.0, -h / 2.0, r, 30, -90.0, 0.0));
                    pts.push([-w / 2.0 - r, r]);
                    pts.extend(generate_circle_points(w / 2.0 + r * 2.0, -r, r, 20, -180.0, -270.0));
                    pts.extend(generate_circle_points(w / 2.0 + r * 2.0, r, r, 20, -90.0, -180.0));
                    pts.push([-w / 2.0 - r, -h / 2.0]);
                    pts.extend(generate_circle_points(w / 2.0, h / 2.0, r, 20, 0.0, 90.0));
                    let mut rect: Vec<[f64; 2]> = vec![[w / 2.0, -h / 2.0 - r], [-w / 2.0, -h / 2.0 - r]];
                    rect.extend(generate_circle_points(w / 2.0, -h / 2.0, r, 20, -90.0, 0.0));
                    rect.push([-w / 2.0 - r, -r]);
                    rect.extend(generate_circle_points(w / 2.0 + w * 0.1, -r, r, 20, -180.0, -270.0));
                    rect.extend(generate_circle_points(w / 2.0 + w * 0.1, r, r, 20, -90.0, -180.0));
                    rect.push([-w / 2.0 - r, h / 2.0]);
                    rect.extend(generate_circle_points(w / 2.0, h / 2.0, r, 20, 0.0, 90.0));
                    rect.push([-w / 2.0, h / 2.0 + r]);
                    rect.push([w / 2.0, h / 2.0 + r]);
                    let _ = write!(s, r#"<g class="text" transform="translate({}, 0)">"#, round(r));
                    emit_brace(s, &pts, true, "");
                    emit_brace(s, &rect, false, r#" stroke-opacity="0""#);
                    s.push_str("</g>");
                }
                NodeShape::BraceRight => {
                    let mut pts: Vec<[f64; 2]> = Vec::new();
                    pts.extend(generate_circle_points_pos(w / 2.0, -h / 2.0, r, 20, -90.0, 0.0));
                    pts.push([w / 2.0 + r, -r]);
                    pts.extend(generate_circle_points_pos(w / 2.0 + r * 2.0, -r, r, 20, -180.0, -270.0));
                    pts.extend(generate_circle_points_pos(w / 2.0 + r * 2.0, r, r, 20, -90.0, -180.0));
                    pts.push([w / 2.0 + r, h / 2.0]);
                    pts.extend(generate_circle_points_pos(w / 2.0, h / 2.0, r, 20, 0.0, 90.0));
                    let mut rect: Vec<[f64; 2]> = vec![[-w / 2.0, -h / 2.0 - r], [w / 2.0, -h / 2.0 - r]];
                    rect.extend(generate_circle_points_pos(w / 2.0, -h / 2.0, r, 20, -90.0, 0.0));
                    rect.push([w / 2.0 + r, -r]);
                    rect.extend(generate_circle_points_pos(w / 2.0 + r * 2.0, -r, r, 20, -180.0, -270.0));
                    rect.extend(generate_circle_points_pos(w / 2.0 + r * 2.0, r, r, 20, -90.0, -180.0));
                    rect.push([w / 2.0 + r, h / 2.0]);
                    rect.extend(generate_circle_points_pos(w / 2.0, h / 2.0, r, 20, 0.0, 90.0));
                    rect.push([w / 2.0, h / 2.0 + r]);
                    rect.push([-w / 2.0, h / 2.0 + r]);
                    let _ = write!(s, r#"<g class="text" transform="translate({}, 0)">"#, round(-r));
                    emit_brace(s, &pts, true, "");
                    emit_brace(s, &rect, false, r#" stroke-opacity="0""#);
                    s.push_str("</g>");
                }
                _ => {
                    // Both braces (curlyBraces): right brace, left brace, rect.
                    let mut left: Vec<[f64; 2]> = Vec::new();
                    left.extend(generate_circle_points(w / 2.0, -h / 2.0, r, 30, -90.0, 0.0));
                    left.push([-w / 2.0 - r, r]);
                    left.extend(generate_circle_points(w / 2.0 + r * 2.0, -r, r, 20, -180.0, -270.0));
                    left.extend(generate_circle_points(w / 2.0 + r * 2.0, r, r, 20, -90.0, -180.0));
                    left.push([-w / 2.0 - r, -h / 2.0]);
                    left.extend(generate_circle_points(w / 2.0, h / 2.0, r, 20, 0.0, 90.0));
                    let mut right: Vec<[f64; 2]> = Vec::new();
                    right.extend(generate_circle_points(-w / 2.0 + r + r / 2.0, -h / 2.0, r, 20, -90.0, -180.0));
                    right.push([w / 2.0 - r / 2.0, r]);
                    right.extend(generate_circle_points(-w / 2.0 - r / 2.0, -r, r, 20, 0.0, 90.0));
                    right.extend(generate_circle_points(-w / 2.0 - r / 2.0, r, r, 20, -90.0, 0.0));
                    right.push([w / 2.0 - r / 2.0, -r]);
                    right.extend(generate_circle_points(-w / 2.0 + r + r / 2.0, h / 2.0, r, 30, -180.0, -270.0));
                    let mut rect: Vec<[f64; 2]> = vec![[w / 2.0, -h / 2.0 - r], [-w / 2.0, -h / 2.0 - r]];
                    rect.extend(generate_circle_points(w / 2.0, -h / 2.0, r, 20, -90.0, 0.0));
                    rect.push([-w / 2.0 - r, -r]);
                    rect.extend(generate_circle_points(w / 2.0 + r * 2.0, -r, r, 20, -180.0, -270.0));
                    rect.extend(generate_circle_points(w / 2.0 + r * 2.0, r, r, 20, -90.0, -180.0));
                    rect.push([-w / 2.0 - r, h / 2.0]);
                    rect.extend(generate_circle_points(w / 2.0, h / 2.0, r, 20, 0.0, 90.0));
                    rect.push([-w / 2.0, h / 2.0 + r]);
                    rect.push([w / 2.0 - r - r / 2.0, h / 2.0 + r]);
                    rect.extend(generate_circle_points(-w / 2.0 + r + r / 2.0, -h / 2.0, r, 20, -90.0, -180.0));
                    rect.push([w / 2.0 - r / 2.0, r]);
                    rect.extend(generate_circle_points(-w / 2.0 - r / 2.0, -r, r, 20, 0.0, 90.0));
                    rect.extend(generate_circle_points(-w / 2.0 - r / 2.0, r, r, 20, -90.0, 0.0));
                    rect.push([w / 2.0 - r / 2.0, -r]);
                    rect.extend(generate_circle_points(-w / 2.0 + r + r / 2.0, h / 2.0, r, 30, -180.0, -270.0));
                    let _ = write!(s, r#"<g class="text" transform="translate({}, 0)">"#, round(r - r / 4.0));
                    emit_brace(s, &right, true, "");
                    emit_brace(s, &left, true, "");
                    emit_brace(s, &rect, false, r#" stroke-opacity="0""#);
                    s.push_str("</g>");
                }
            }
        }
    }
}

/// The classic-look `roughr` options for a node shape: solid fill in the theme
/// node colour, theme border stroke, and the given `roughness` (0 for the
/// classic look — its fill path is then the exact shape vertices). Mirrors
/// mermaid's `userNodeOverrides(node, {})` + `roughness=0; fillStyle="solid"`.
fn rough_options(roughness: f64, pal: &Palette) -> roughr::Options {
    let gen = roughr::Generator::new();
    let mut o = gen.default_options();
    o.roughness = roughness;
    o.fill = Some(pal.node_bkg.to_string());
    // Classic (roughness 0) fills solid — the fill path is then the exact shape
    // vertices, matching mermaid. The hand-drawn look (roughness > 0) fills with
    // rough.js's hachure (parallel sketch lines), matching mermaid's handDrawn.
    o.fill_style = if roughness > 0.0 { "hachure" } else { "solid" }.to_string();
    o.stroke = pal.node_border.to_string();
    o.bowing = 1.0;
    o.seed = 1;
    // Match mermaid's `userNodeOverrides` so the hachure (hand-drawn fill) has
    // the same line spacing/weight as the oracle. Without these, roughr's
    // default hachureGap (~4) equals the 4px line width, so the sketch lines
    // touch and merge into a solid fill instead of reading as hand-drawn.
    o.fill_weight = 4.0;
    o.hachure_gap = 5.2;
    o.hachure_angle = -41.0;
    o.stroke_width = 1.3;
    o
}

/// `true` when the given roughness selects the hand-drawn look (roughness > 0).
fn is_hand_drawn(roughness: f64) -> bool {
    roughness > 0.0
}

/// Look up a CSS property value in a `;`-separated decl string like
/// `"fill:#888 !important;stroke:#001f3f !important"`, stripping any
/// `!important`/whitespace. Returns `None` when the property is absent.
fn css_value(style: &str, prop: &str) -> Option<String> {
    for decl in style.split(';') {
        let Some((k, v)) = decl.split_once(':') else { continue };
        if k.trim() == prop {
            return Some(v.replace("!important", "").trim().to_string());
        }
    }
    None
}

/// The resolved (fill, stroke, stroke-width) a hand-drawn shape should paint,
/// taken from the node's inline shape style (classDef / `style` colours) and
/// falling back to the theme node colours. mermaid bakes these into the rough
/// paths (the fill colour draws the hachure lines, the stroke the outline)
/// rather than onto the group's `style`, so per-node colours need threading
/// into the drawable emission here.
fn hand_drawn_colors(shape_style: &str, pal: &Palette) -> (String, String, String) {
    let fill = css_value(shape_style, "fill").unwrap_or_else(|| pal.node_bkg.to_string());
    let stroke = css_value(shape_style, "stroke").unwrap_or_else(|| pal.node_border.to_string());
    let sw = css_value(shape_style, "stroke-width")
        .map(|w| w.trim_end_matches("px").trim().to_string())
        .unwrap_or_else(|| "1.3".to_string());
    (fill, stroke, sw)
}

/// Emit a rough drawable for a hand-drawn shape with explicit resolved colours
/// (the hachure fill colour, the outline stroke, and its width).
fn emit_hd_drawable(s: &mut String, drawable: &roughr::Drawable, evenodd: bool, fill: &str, stroke: &str, sw: &str) {
    emit_drawable(s, drawable, evenodd, "", Some(fill), stroke, sw);
}

/// `true` for the fixed-size icon shapes mermaid renders with no label text.
/// Their handlers (`forkJoin`, `stateStart`, `filledCircle`, `stateEnd`,
/// `crossedCircle`, `hourglass`, `lightningBolt`) blank the label and skip
/// `labelHelper`, so the node group emits no `g.label` child.
fn is_labelless(shape: NodeShape) -> bool {
    matches!(
        shape,
        NodeShape::Fork
            | NodeShape::SmallCircle
            | NodeShape::FilledCircle
            | NodeShape::FramedCircle
            | NodeShape::CrossedCircle
            | NodeShape::Hourglass
            | NodeShape::LightningBolt
    )
}

/// The ` style="…"` attribute that mermaid's hand-drawn shape groups always
/// carry (empty when the node is unstyled: `style=""`).
fn hd_style(shape_style: &str) -> String {
    format!(r#" style="{}""#, escape(shape_style))
}

/// Emit a hand-drawn polygon shape (rhombus / hexagon / slanted shapes): a
/// `<g transform=… style=…>` wrapping the rough hachure-fill + outline paths,
/// built from the same vertex array the classic look uses.
fn emit_hd_polygon(s: &mut String, points: &[[f64; 2]], tx: f64, ty: f64, shape_style: &str, roughness: f64, pal: &Palette) {
    let o = rough_options(roughness, pal);
    let drawable = roughr::Generator::new().polygon(points, &o);
    let (fill, stroke, sw) = hand_drawn_colors(shape_style, pal);
    let _ = write!(
        s,
        r#"<g transform="translate({}, {})"{}>"#,
        round(tx),
        round(ty),
        hd_style(shape_style),
    );
    emit_hd_drawable(s, &drawable, false, &fill, &stroke, &sw);
    s.push_str("</g>");
}

/// The cylinder *silhouette* path `d` in local (top-left origin) coordinates: a
/// single closed loop tracing the top back arc, the right side, the bottom
/// front arc, and the left side. Unlike the classic full outline (which draws
/// the complete top ellipse as two arcs), this has no self-intersecting cap
/// sub-loop, so roughr's hachure fills it as continuous diagonals across the
/// whole body — matching mermaid's handDrawn cylinder. `ry`/`rx` are the cap
/// radii, `w` the width, `body_h` the straight body height.
fn cylinder_silhouette(ry: f64, rx: f64, w: f64, body_h: f64) -> String {
    format!(
        "M0,{ry} a{rx},{ry} 0,0,1 {w},0 l0,{body} a{rx},{ry} 0,0,1 {nw},0 l0,{nbody}",
        ry = round(ry),
        rx = round(rx),
        w = round(w),
        nw = round(-w),
        body = round(body_h),
        nbody = round(-body_h),
    )
}

/// An SVG rounded-rectangle path `d`, used by the hand-drawn rounded rectangle
/// and stadium (which is a rounded rect with `r = height/2`).
fn rounded_rect_path(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    format!(
        "M{x},{yr} A{r},{r} 0 0 1 {xr},{y} L{xwr},{y} A{r},{r} 0 0 1 {xw},{yr} \
         L{xw},{yhr} A{r},{r} 0 0 1 {xwr},{yh} L{xr},{yh} A{r},{r} 0 0 1 {x},{yhr} Z",
        x = round(x),
        y = round(y),
        r = round(r),
        xr = round(x + r),
        xwr = round(x + w - r),
        xw = round(x + w),
        yr = round(y + r),
        yhr = round(y + h - r),
        yh = round(y + h),
    )
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
fn emit_rough_drawable(s: &mut String, drawable: &roughr::Drawable, evenodd: bool, st: &str, pal: &Palette) {
    emit_drawable(s, drawable, evenodd, st, Some(pal.node_bkg), pal.node_border, "1.3");
}

/// General form of [`emit_rough_drawable`] with explicit colours. When `fill` is
/// `Some`, a fill `<path>` is emitted (even if the drawable has no fill area, in
/// which case its `d` is empty — matching rough.js, which still emits the element
/// whenever `options.fill` is set); when `None`, no fill element is emitted
/// (rough.js creates none for `fill: "none"`). `sw` is the stroke-width attribute.
fn emit_drawable(
    s: &mut String,
    drawable: &roughr::Drawable,
    evenodd: bool,
    st: &str,
    fill: Option<&str>,
    stroke: &str,
    sw: &str,
) {
    if let Some(fc) = fill {
        let fill_d = drawable.fill_path(None);
        if drawable.options.fill_style == "hachure" {
            // Hand-drawn look: the fill is a hachure sketch (parallel lines),
            // so it renders as a *stroke* in the fill colour with no area fill.
            // fill-rule is irrelevant (fill="none") and mermaid omits it.
            let _ = write!(
                s,
                r##"<path d="{fill_d}" stroke="{fc}" stroke-width="4" fill="none" stroke-dasharray="0 0"{st}/>"##,
            );
        } else {
            let rule = if evenodd { r#" fill-rule="evenodd""# } else { "" };
            let _ = write!(
                s,
                r##"<path d="{fill_d}" stroke="none" stroke-width="0" fill="{fc}"{rule}{st}/>"##,
            );
        }
    }
    let stroke_d = drawable.stroke_path(None);
    let _ = write!(
        s,
        r##"<path d="{stroke_d}" stroke="{stroke}" stroke-width="{sw}" fill="none" stroke-dasharray="0 0"{st}/>"##,
    );
}

/// Emit a `roughr` [`Drawable`] as mermaid's `mergePaths` output: a `<g>` holding
/// a single `<path>` whose `d` is the fill path followed by the stroke path, with
/// the fill and stroke attributes merged onto that one element (used by the
/// stacked-rectangle / multi-process handler for the classic look).
///
/// [`Drawable`]: roughr::Drawable
fn emit_merged(s: &mut String, drawable: &roughr::Drawable, st: &str, pal: &Palette) {
    let fill = drawable.fill_path(None);
    let stroke = drawable.stroke_path(None);
    let d = if fill.is_empty() { stroke } else { format!("{fill} {stroke}") };
    let _ = write!(
        s,
        r##"<g><path d="{d}" fill="{fill_c}" fill-opacity="1" stroke="{stroke_c}" stroke-width="1.3" stroke-opacity="1"{st}/></g>"##,
        fill_c = pal.node_bkg,
        stroke_c = pal.node_border,
    );
}

/// Port of mermaid's base `generateCirclePoints`: sample `n` points on a circle
/// arc (degrees), **negating** each coordinate (mermaid's base variant pushes
/// `{-x, -y}`). Used by delay / curved-trapezoid / notched-pentagon families.
fn generate_circle_points(
    cx: f64,
    cy: f64,
    r: f64,
    n: usize,
    start_deg: f64,
    end_deg: f64,
) -> Vec<[f64; 2]> {
    let start = start_deg * std::f64::consts::PI / 180.0;
    let end = end_deg * std::f64::consts::PI / 180.0;
    let step = (end - start) / (n as f64 - 1.0);
    (0..n)
        .map(|i| {
            let a = start + i as f64 * step;
            [-(cx + r * a.cos()), -(cy + r * a.sin())]
        })
        .collect()
}

/// Non-negating circle-point sampler (mermaid's `generateCirclePoints` variant
/// used by the right curly brace, which pushes `{x, y}` rather than `{-x, -y}`).
fn generate_circle_points_pos(
    cx: f64,
    cy: f64,
    r: f64,
    n: usize,
    start_deg: f64,
    end_deg: f64,
) -> Vec<[f64; 2]> {
    let start = start_deg * std::f64::consts::PI / 180.0;
    let end = end_deg * std::f64::consts::PI / 180.0;
    let step = (end - start) / (n as f64 - 1.0);
    (0..n)
        .map(|i| {
            let a = start + i as f64 * step;
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect()
}

/// Port of mermaid's `generateArcPoints`: sample 20 points along the elliptical
/// arc between `(x1,y1)` and `(x2,y2)` with radii `rx,ry`. Used by bow-tie rect.
fn generate_arc_points(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    rx: f64,
    ry: f64,
    clockwise: bool,
) -> Vec<[f64; 2]> {
    let num = 20;
    let mid_x = (x1 + x2) / 2.0;
    let mid_y = (y1 + y2) / 2.0;
    let angle = (y2 - y1).atan2(x2 - x1);
    let dx = (x2 - x1) / 2.0;
    let dy = (y2 - y1) / 2.0;
    let distance = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt();
    let scaled = (1.0 - distance * distance).max(0.0).sqrt();
    let sign = if clockwise { -1.0 } else { 1.0 };
    let center_x = mid_x + scaled * ry * angle.sin() * sign;
    let center_y = mid_y - scaled * rx * angle.cos() * sign;
    let start = ((y1 - center_y) / ry).atan2((x1 - center_x) / rx);
    let end = ((y2 - center_y) / ry).atan2((x2 - center_x) / rx);
    let mut range = end - start;
    if clockwise && range < 0.0 {
        range += 2.0 * std::f64::consts::PI;
    }
    if !clockwise && range > 0.0 {
        range -= 2.0 * std::f64::consts::PI;
    }
    (0..num)
        .map(|i| {
            let t = i as f64 / (num as f64 - 1.0);
            let a = start + t * range;
            [center_x + rx * a.cos(), center_y + ry * a.sin()]
        })
        .collect()
}

/// mermaid's arc sagitta (chord depth) used to size the bow-tie's concave sides.
fn arc_sagitta(chord: f64, rx: f64, ry: f64) -> f64 {
    let (major, minor) = if rx >= ry { (rx, ry) } else { (ry, rx) };
    minor * (1.0 - (1.0 - (chord / major / 2.0).powi(2)).sqrt())
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
    // Vertical cylinders: dagre routes to the (full-height) bounding rect, but the
    // top/bottom are elliptical caps, so an edge aimed near a cap ends above/below
    // the drawn outline. Port mermaid's cylinder `intersect`: rect intersection,
    // then push the endpoint onto the cap ellipse in the cap region.
    if let NodeShape::Cylinder | NodeShape::LinedCylinder = node.shape {
        let (hw, hh) = (node.width / 2.0, node.height / 2.0);
        let ry = crate::layout::cylinder_ry(node.width);
        let (dx, dy) = (toward.0 - c.0, toward.1 - c.1);
        if dx == 0.0 && dy == 0.0 {
            return None;
        }
        // intersect.rect: crossing of the ray with the node's bounding rectangle.
        let (mut sw, mut sh) = (hw, hh);
        let (sx, sy);
        if dy.abs() * hw > dx.abs() * hh {
            if dy < 0.0 {
                sh = -sh;
            }
            sx = if dy == 0.0 { 0.0 } else { sh * dx / dy };
            sy = sh;
        } else {
            if dx < 0.0 {
                sw = -sw;
            }
            sx = sw;
            sy = if dx == 0.0 { 0.0 } else { sw * dy / dx };
        }
        let mut y = sy;
        // In the cap region (hit the flat top/bottom, or the side beyond the body)
        // drop the point onto the ellipse: y += ry - sqrt(ry²(1 - x²/rx²)).
        if hw != 0.0 && (sx.abs() < hw || (sx.abs() == hw && sy.abs() > hh - ry)) {
            let mut yv = ry * ry * (1.0 - sx * sx / (hw * hw));
            if yv > 0.0 {
                yv = yv.sqrt();
            }
            yv = ry - yv;
            if dy > 0.0 {
                yv = -yv;
            }
            y += yv;
        }
        return Some((c.0 + sx, c.1 + y));
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
