//! Geometry assignment. Consumes an [`ir::Diagram`] and produces a laid-out
//! diagram with concrete coordinates ready for rendering.
//!
//! The current flowchart layout is a **placeholder**: a simple longest-path
//! layered assignment (rank = longest path from a root, x = order within rank).
//! It establishes the [`LaidOut`] data shape and a working end-to-end pipeline.
//! It will be replaced by a dagre-compatible layered layout (rank → order →
//! coordinate assignment) so that structural SVG comparison against the mermaid
//! CLI becomes meaningful. See `ATTRIBUTION.md` for the dagre reference.

use crate::ir::{Diagram, Direction, Flowchart, NodeShape};
use crate::Result;

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

/// An edge routed as a polyline through `points` (center to center for now).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedEdge {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
    pub points: Vec<(f64, f64)>,
}

// --- Tunable placeholder layout constants (rough mermaid-like defaults). ---
// mermaid's single-line node box is 49px tall; matching it makes the rank-axis
// coordinates line up with mermaid (margin 8 + height 49 + ranksep 50).
const NODE_HEIGHT: f64 = 49.0;
const CHAR_WIDTH: f64 = 9.0;
const NODE_PADDING_X: f64 = 16.0;
const RANK_SEP: f64 = 50.0;
const NODE_SEP: f64 = 40.0;
const MARGIN: f64 = 8.0;

/// Lay out a diagram.
pub fn layout(diagram: &Diagram) -> Result<LaidOut> {
    match diagram {
        Diagram::Flowchart(f) => Ok(LaidOut::Flowchart(layout_flowchart(f))),
    }
}

fn node_size(label: &str) -> (f64, f64) {
    let w = (label.chars().count() as f64 * CHAR_WIDTH) + 2.0 * NODE_PADDING_X;
    (w.max(40.0), NODE_HEIGHT)
}

fn layout_flowchart(chart: &Flowchart) -> LaidOutFlowchart {
    let n = chart.nodes.len();
    let ranks = longest_path_ranks(chart);
    let max_rank = ranks.iter().copied().max().unwrap_or(0);

    // Group node indices by rank, preserving input order within a rank.
    let mut by_rank: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (idx, &r) in ranks.iter().enumerate() {
        by_rank[r].push(idx);
    }

    let horizontal = matches!(chart.direction, Direction::LeftRight | Direction::RightLeft);

    let mut placed: Vec<PlacedNode> = Vec::with_capacity(n);
    placed.resize(
        n,
        PlacedNode {
            id: String::new(),
            label: String::new(),
            shape: NodeShape::Rectangle,
            cx: 0.0,
            cy: 0.0,
            width: 0.0,
            height: 0.0,
        },
    );

    let mut diagram_w: f64 = 0.0;
    let mut diagram_h: f64 = 0.0;

    // Place rank by rank. "Along" is the within-rank axis; "depth" is the
    // rank axis. We map those to x/y based on direction.
    let mut depth_cursor = MARGIN;
    for (r, members) in by_rank.iter().enumerate() {
        let _ = r;
        let mut along_cursor = MARGIN;
        let mut rank_thickness: f64 = 0.0;
        for &idx in members {
            let node = &chart.nodes[idx];
            let (w, h) = node_size(&node.label);
            let (depth_extent, along_extent) = if horizontal { (w, h) } else { (h, w) };
            rank_thickness = rank_thickness.max(depth_extent);

            let along_center = along_cursor + along_extent / 2.0;
            let depth_center = depth_cursor + depth_extent / 2.0;
            let (cx, cy) = if horizontal {
                (depth_center, along_center)
            } else {
                (along_center, depth_center)
            };

            placed[idx] = PlacedNode {
                id: node.id.clone(),
                label: node.label.clone(),
                shape: node.shape,
                cx,
                cy,
                width: w,
                height: h,
            };
            along_cursor += along_extent + NODE_SEP;
            diagram_w = diagram_w.max(cx + w / 2.0 + MARGIN);
            diagram_h = diagram_h.max(cy + h / 2.0 + MARGIN);
        }
        depth_cursor += rank_thickness + RANK_SEP;
    }

    let edges = chart
        .edges
        .iter()
        .map(|e| {
            let a = &placed[e.from];
            let b = &placed[e.to];
            PlacedEdge {
                from: e.from,
                to: e.to,
                label: e.label.clone(),
                points: vec![(a.cx, a.cy), (b.cx, b.cy)],
            }
        })
        .collect();

    LaidOutFlowchart {
        direction: chart.direction,
        width: diagram_w.max(MARGIN * 2.0),
        height: diagram_h.max(MARGIN * 2.0),
        nodes: placed,
        edges,
    }
}

/// Longest-path layering: each node's rank is the longest chain of edges
/// leading into it. Cycles are broken implicitly by the bounded iteration.
fn longest_path_ranks(chart: &Flowchart) -> Vec<usize> {
    let n = chart.nodes.len();
    let mut rank = vec![0usize; n];
    // Relax edges up to n times (sufficient for a DAG; bounded for cycles).
    for _ in 0..n {
        let mut changed = false;
        for e in &chart.edges {
            if rank[e.to] < rank[e.from] + 1 {
                rank[e.to] = rank[e.from] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    rank
}
