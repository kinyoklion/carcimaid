//! Geometry assignment. Consumes an [`ir::Diagram`] and produces a laid-out
//! diagram with concrete coordinates ready for rendering.
//!
//! Flowchart layout delegates to the [`dagre`] crate — a Rust port of the same
//! dagre algorithm mermaid uses — driven with mermaid's parameters (nodesep /
//! ranksep 50, margin 8, network-simplex ranking). With our DejaVu-based node
//! sizes as input, this reproduces mermaid's node coordinates, edge waypoints,
//! and overall dimensions. See `ATTRIBUTION.md` for the dagre/font references.

use crate::ir::{Diagram, Direction, Flowchart, NodeShape};
use crate::Result;

use dagre::graph::{Graph, GraphOptions};
use dagre::layout::types::{EdgeLabel, GraphLabel, LayoutOptions, NodeLabel, RankDir};
use dagre::layout::layout as dagre_layout;

/// A laid-out diagram: geometry plus enough of the model to render.
#[derive(Debug, Clone, PartialEq)]
pub enum LaidOut {
    Flowchart(LaidOutFlowchart),
}

/// A flowchart with concrete geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutFlowchart {
    pub direction: Direction,
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<PlacedNode>,
    pub edges: Vec<PlacedEdge>,
    pub clusters: Vec<PlacedCluster>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
}

/// A subgraph cluster with concrete geometry (its bounding box).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedCluster {
    pub id: String,
    pub title: String,
    /// Center of the cluster box.
    pub cx: f64,
    pub cy: f64,
    pub width: f64,
    pub height: f64,
}

/// A node with a center position and a box size.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedNode {
    pub id: String,
    pub label: String,
    pub shape: NodeShape,
    /// Center of the node box.
    pub cx: f64,
    pub cy: f64,
    pub width: f64,
    pub height: f64,
}

/// An edge routed as a polyline through `points` (dagre's waypoints, from the
/// source border through any bend points to the target border).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedEdge {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
    pub points: Vec<(f64, f64)>,
    /// Whether the edge has an arrowhead at the `to` end.
    pub arrow: bool,
    /// Dagre-computed label position (center), if the edge has a label.
    pub label_pos: Option<(f64, f64)>,
}

// --- mermaid layout parameters (its flowchart defaults). ---
const NODE_HEIGHT: f64 = 49.0;
const FONT_SIZE: f64 = 16.0;
/// Extra height per wrapped line (mermaid's 1.1em row step at 16px).
const LINE_SPACING: f64 = 17.6;
/// Single-line box height for polygon shapes (bbox.height + padding).
const POLY_H: f64 = 34.0;
const NODE_SEP: f64 = 50.0;
const RANK_SEP: f64 = 50.0;
const EDGE_SEP: f64 = 20.0;
const MARGIN: f64 = 8.0;

/// Lay out a diagram.
pub fn layout(diagram: &Diagram) -> Result<LaidOut> {
    match diagram {
        Diagram::Flowchart(f) => Ok(LaidOut::Flowchart(layout_flowchart(f))),
    }
}

/// Node box size from the label's measured text width plus shape-dependent
/// padding, derived empirically from mermaid's output (plain rect +60, rounded
/// +30). Other shapes are approximated pending dedicated shape sizing.
fn node_size(label: &str, shape: NodeShape) -> (f64, f64) {
    // Wrap the label the way mermaid does; the widest line drives node width and
    // the line count drives extra height (1.1em ≈ 17.6px per additional line).
    let lines = crate::text::wrap_label(label, crate::text::WRAP_WIDTH, FONT_SIZE);
    let line_count = lines.len().max(1) as f64;
    let text_w = lines
        .iter()
        .map(|l| crate::text::line_width(l, FONT_SIZE))
        .fold(0.0_f64, f64::max);
    let extra = (line_count - 1.0) * LINE_SPACING;

    match shape {
        NodeShape::Rectangle => (text_w + 60.0, NODE_HEIGHT + extra),
        NodeShape::RoundedRectangle => (text_w + 30.0, NODE_HEIGHT + extra),
        // Rhombus (mermaid `question`): a square diamond of side s = w + h.
        // Empirically the additive offset over our measured text width is 49 for
        // a single line; extra lines grow it. The box is s × s.
        NodeShape::Rhombus => {
            let s = text_w + 49.0 + extra;
            (s, s)
        }
        // Circle: 2r ≈ measured text width + 14.8; box is 2r × 2r.
        NodeShape::Circle => {
            let s = text_w + 14.8 + extra;
            (s, s)
        }
        // Polygon shapes: box height 34 (bbox.height + padding) for a single
        // line; widths calibrated per shape (bbox.width + shape padding). See
        // render::render_shape for the matching point geometry.
        NodeShape::Hexagon => {
            let h = POLY_H + extra;
            (text_w + h / 2.0 + 14.72, h) // w = bbox.w + 2*(h/4) + pad
        }
        NodeShape::Subroutine => {
            let h = POLY_H + extra;
            (text_w + 16.0 + 14.65, h) // + 2*FRAME_WIDTH(8) + pad
        }
        // Slanted shapes overflow their label box by h/2 on each side, so the
        // dagre box (and viewBox/centering) must include that: width = label
        // width + pad + h. The renderer recovers the inner width as width - h.
        NodeShape::Parallelogram | NodeShape::LeanLeft => {
            let h = POLY_H + extra;
            (text_w + 14.29 + h, h)
        }
        NodeShape::Trapezoid | NodeShape::InvTrapezoid => {
            let h = POLY_H + extra;
            (text_w + 13.65 + h, h)
        }
        // TODO: stadium/cylinder are path shapes; still approximated as rects.
        NodeShape::Stadium | NodeShape::Cylinder => (text_w + 60.0, NODE_HEIGHT + extra),
    }
}

fn rank_dir(dir: Direction) -> RankDir {
    match dir {
        Direction::TopBottom => RankDir::TB,
        Direction::BottomTop => RankDir::BT,
        Direction::LeftRight => RankDir::LR,
        Direction::RightLeft => RankDir::RL,
    }
}

fn layout_flowchart(chart: &Flowchart) -> LaidOutFlowchart {
    // Our node sizes (used both as dagre input and as the rendered box sizes).
    let sizes: Vec<(f64, f64)> = chart
        .nodes
        .iter()
        .map(|n| node_size(&n.label, n.shape))
        .collect();

    if chart.nodes.is_empty() {
        return LaidOutFlowchart {
            direction: chart.direction,
            width: MARGIN * 2.0,
            height: MARGIN * 2.0,
            nodes: Vec::new(),
            edges: Vec::new(),
            clusters: Vec::new(),
            title: chart.title.clone(),
            acc_title: chart.acc_title.clone(),
            acc_descr: chart.acc_descr.clone(),
        };
    }

    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });

    for (i, &(w, h)) in sizes.iter().enumerate() {
        g.set_node(
            i.to_string(),
            Some(NodeLabel { width: w, height: h, ..Default::default() }),
        );
    }
    // Register subgraph clusters and the compound parent relationships.
    for ci in 0..chart.subgraphs.len() {
        g.set_node(cluster_key(ci), Some(NodeLabel::default()));
    }
    for (ci, sg) in chart.subgraphs.iter().enumerate() {
        if let Some(p) = sg.parent {
            g.set_parent(&cluster_key(ci), Some(&cluster_key(p)));
        }
    }
    for (ni, node) in chart.nodes.iter().enumerate() {
        if let Some(ci) = node.subgraph {
            g.set_parent(&ni.to_string(), Some(&cluster_key(ci)));
        }
    }
    for (i, e) in chart.edges.iter().enumerate() {
        // Reserve space for an edge label so dagre routes around it, matching
        // mermaid. Unlabelled edges contribute no label box.
        let (lw, lh) = match &e.label {
            Some(l) => (crate::text::measure_width(l, FONT_SIZE), crate::text::line_height(FONT_SIZE)),
            None => (0.0, 0.0),
        };
        g.set_edge(
            e.from.to_string(),
            e.to.to_string(),
            Some(EdgeLabel { width: lw, height: lh, ..Default::default() }),
            Some(i.to_string().as_str()),
        );
    }

    dagre_layout(
        &mut g,
        Some(LayoutOptions {
            rankdir: rank_dir(chart.direction),
            nodesep: NODE_SEP,
            ranksep: RANK_SEP,
            edgesep: EDGE_SEP,
            marginx: MARGIN,
            marginy: MARGIN,
            tie_keep_first: true,
            ..Default::default()
        }),
    );

    let nodes: Vec<PlacedNode> = chart
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let placed = g.node(&i.to_string());
            let (w, h) = sizes[i];
            PlacedNode {
                id: node.id.clone(),
                label: node.label.clone(),
                shape: node.shape,
                cx: placed.and_then(|n| n.x).unwrap_or(0.0),
                cy: placed.and_then(|n| n.y).unwrap_or(0.0),
                width: w,
                height: h,
            }
        })
        .collect();

    let edges: Vec<PlacedEdge> = chart
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let el = g.edge(&e.from.to_string(), &e.to.to_string(), Some(&i.to_string()));
            let points = el
                .map(|el| el.points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| vec![(nodes[e.from].cx, nodes[e.from].cy), (nodes[e.to].cx, nodes[e.to].cy)]);
            // mermaid places the edge label at the midpoint of the edge path by
            // arc length (not at dagre's reserved label slot, which sits off the
            // path).
            let label_pos = e.label.as_ref().map(|_| midpoint_by_length(&points));
            PlacedEdge {
                from: e.from,
                to: e.to,
                label: e.label.clone(),
                points,
                arrow: e.arrow,
                label_pos,
            }
        })
        .collect();

    let clusters: Vec<PlacedCluster> = chart
        .subgraphs
        .iter()
        .enumerate()
        .map(|(ci, sg)| {
            let n = g.node(&cluster_key(ci));
            PlacedCluster {
                id: sg.id.clone(),
                title: sg.title.clone(),
                cx: n.and_then(|n| n.x).unwrap_or(0.0),
                cy: n.and_then(|n| n.y).unwrap_or(0.0),
                width: n.map(|n| n.width).unwrap_or(0.0),
                height: n.map(|n| n.height).unwrap_or(0.0),
            }
        })
        .collect();

    let (width, height) = g
        .graph_label::<GraphLabel>()
        .map(|gl| (gl.width, gl.height))
        .unwrap_or((MARGIN * 2.0, MARGIN * 2.0));

    LaidOutFlowchart {
        direction: chart.direction,
        width,
        height,
        nodes,
        edges,
        clusters,
        title: chart.title.clone(),
        acc_title: chart.acc_title.clone(),
        acc_descr: chart.acc_descr.clone(),
    }
}

/// dagre node key for the subgraph at index `i` (kept distinct from node keys,
/// which are plain indices).
fn cluster_key(i: usize) -> String {
    format!("cluster{i}")
}

/// The point at 50% of a polyline's arc length — where mermaid places edge labels.
fn midpoint_by_length(points: &[(f64, f64)]) -> (f64, f64) {
    match points {
        [] => (0.0, 0.0),
        [p] => *p,
        _ => {
            let seg = |a: (f64, f64), b: (f64, f64)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            let total: f64 = points.windows(2).map(|w| seg(w[0], w[1])).sum();
            let mut remaining = total / 2.0;
            for w in points.windows(2) {
                let d = seg(w[0], w[1]);
                if d >= remaining {
                    let t = if d > 0.0 { remaining / d } else { 0.0 };
                    return (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
                }
                remaining -= d;
            }
            *points.last().unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn flowchart(src: &str) -> LaidOutFlowchart {
        match layout(&parser::parse(src).unwrap()).unwrap() {
            LaidOut::Flowchart(f) => f,
        }
    }

    #[test]
    fn rhombus_and_circle_are_square() {
        let f = flowchart("flowchart TD\n A{Ready} --> B((Go))");
        for n in &f.nodes {
            assert!((n.width - n.height).abs() < 1e-9, "{} not square: {}x{}", n.id, n.width, n.height);
        }
    }

    #[test]
    fn simple_chain_matches_mermaid_coordinates() {
        // mermaid renders `A[Start] --> B[Middle] --> C[End]` with all three
        // nodes centred on x=64.43 at y=32.5/131.5/230.5, viewBox 128.86x263.
        let f = flowchart("flowchart TD\n A[Start] --> B[Middle] --> C[End]");
        assert_eq!(f.nodes.len(), 3);
        let approx = |a: f64, b: f64| (a - b).abs() < 0.5;
        for n in &f.nodes {
            assert!(approx(n.cx, 64.43), "cx={} expected ~64.43", n.cx);
        }
        assert!(approx(f.nodes[0].cy, 32.5), "{}", f.nodes[0].cy);
        assert!(approx(f.nodes[1].cy, 131.5), "{}", f.nodes[1].cy);
        assert!(approx(f.nodes[2].cy, 230.5), "{}", f.nodes[2].cy);
        assert!(approx(f.width, 128.86) && approx(f.height, 263.0), "{}x{}", f.width, f.height);
    }
}
