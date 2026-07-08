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
use dagre::layout::layout as dagre_layout;
use dagre::layout::types::{EdgeLabel, LayoutOptions, NodeLabel, RankDir};

pub mod sequence;

/// A laid-out diagram: geometry plus enough of the model to render.
#[derive(Debug, Clone, PartialEq)]
pub enum LaidOut {
    Flowchart(LaidOutFlowchart),
    Sequence(sequence::LaidOutSequence),
}

/// A flowchart with concrete geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutFlowchart {
    pub direction: Direction,
    /// Top-left of the content bounding box (viewBox origin before any title).
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<PlacedNode>,
    pub edges: Vec<PlacedEdge>,
    pub clusters: Vec<PlacedCluster>,
    /// Visual look (propagated from [`crate::ir::Flowchart::look`]); selects the
    /// rough.js roughness the renderer uses for node shapes.
    pub look: crate::ir::Look,
    /// Colour theme (propagated from [`crate::ir::Flowchart::theme`]); selects
    /// the palette the renderer paints default colours from.
    pub theme: crate::ir::Theme,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    /// Absolute render offset of each subgraph's scope (indexed by subgraph).
    /// Separately-laid-out subgraphs render inside a translated `g.root`; node
    /// coordinates are absolute, so the renderer subtracts this to get the
    /// group-relative coordinates mermaid emits. `(0,0)` for inline subgraphs.
    pub scope_offsets: Vec<(f64, f64)>,
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
    /// Index of this subgraph (for scope recursion in the renderer).
    pub sg_index: usize,
    /// Whether this subgraph is laid out separately (transposed direction). Such
    /// clusters render as a nested `g.root` under their parent scope's nodes.
    pub extracted: bool,
    /// The enclosing extracted subgraph (render scope), or `None` for the root.
    pub home: Option<usize>,
    /// mermaid render order: reverse of definition order among siblings, parents
    /// before children (a children-reversed DFS of the subgraph tree).
    pub order: usize,
    /// Inline `style` for the cluster rect (resolved from `style`/`class`).
    pub shape_style: String,
    /// Class names applied (added to the cluster group's `class` attribute).
    pub classes: Vec<String>,
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
    /// The enclosing extracted subgraph (render scope), or `None` for the root.
    pub home: Option<usize>,
    /// Inline `style` for the node shape (resolved from `style`/`class`).
    pub shape_style: String,
    /// Inline `style` for the node label text (color/font).
    pub label_style: String,
    /// Class names applied (added to the node group's `class` attribute).
    pub classes: Vec<String>,
}

/// An edge routed as a polyline through `points` (dagre's waypoints, from the
/// source border through any bend points to the target border).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedEdge {
    pub from: usize,
    pub to: usize,
    /// Source/target ids as used in the edge id (`L_<from>_<to>_0`). Held
    /// explicitly because a subgraph-reference endpoint has no `PlacedNode`.
    pub from_id: String,
    pub to_id: String,
    /// When an endpoint names a subgraph (`X --> Y`), the edge attaches to that
    /// cluster rather than a node; `from`/`to` are then meaningless. dagre has
    /// already clipped the route to the (rectangular) cluster border.
    pub from_cluster: bool,
    pub to_cluster: bool,
    pub label: Option<String>,
    pub points: Vec<(f64, f64)>,
    /// Arrowhead type at the `from`/`to` ends.
    pub arrow_start: crate::ir::ArrowType,
    pub arrow_end: crate::ir::ArrowType,
    /// Dagre-computed label position (center), if the edge has a label.
    pub label_pos: Option<(f64, f64)>,
    /// Inline `style` for the edge path (resolved from `linkStyle`), or empty.
    pub style: String,
    /// Edge stroke colour (from `linkStyle`), which selects a colour-matched
    /// arrow marker. `None` for the default marker.
    pub stroke: Option<String>,
    /// Edge label colour (the last `color:` in the effective linkStyle), painting
    /// the label rect (`color:X !important`) and text (`fill:X !important`).
    pub label_color: Option<String>,
    /// The enclosing extracted subgraph (render scope), or `None` for the root.
    pub home: Option<usize>,
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
        Diagram::Sequence(s) => Ok(LaidOut::Sequence(sequence::layout(s))),
    }
}

/// Node box size from the label's measured text width plus shape-dependent
/// padding, derived empirically from mermaid's output (plain rect +60, rounded
/// +30). Other shapes are approximated pending dedicated shape sizing.
fn node_size(label: &str, shape: NodeShape, dir: Direction) -> (f64, f64) {
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
        // DataStore is a plain rectangle (dashed sides at render time), so it is
        // sized exactly like one.
        NodeShape::Rectangle | NodeShape::DataStore => (text_w + 60.0, NODE_HEIGHT + extra),
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
            (text_w + 15.0 + h, h)
        }
        // Trapezoid (mermaid `trapezoid`): pad 15, h = bbox.h + 15 = 34; the
        // slant adds h/2 each side so dagre width = (bbox + 15) + h.
        NodeShape::Trapezoid => {
            let h = POLY_H + extra;
            (text_w + 15.0 + h, h)
        }
        // Inverted trapezoid (mermaid `inv_trapezoid`): pad 30 (labelPaddingX*2),
        // h = bbox.h + 30 = 49; dagre width = (bbox + 30) + h.
        NodeShape::InvTrapezoid => {
            let h = NODE_HEIGHT + extra;
            (text_w + 30.0 + h, h)
        }
        // Odd/flag shape: rectangle body plus a left notch ~h/2 deep. Approximated
        // as a rect; height matches the polygon family (34 for one line).
        // Odd/flag (mermaid `rect_left_inv_arrow`): rect (bbox + pad 15) with a
        // left notch of depth h/4; dagre width = (bbox + 15) + h/4. The renderer
        // recovers the inner width as width - h/4.
        NodeShape::Odd => {
            let h = POLY_H + extra;
            (text_w + 15.0 + h / 4.0, h)
        }
        // Small start circle: fixed r=7, no label. Box 14×14.
        NodeShape::SmallCircle => (14.0, 14.0),
        // Double circle: outer radius = text_w/2 + padding(15); box = diameter.
        NodeShape::DoubleCircle => {
            let d = text_w + 30.0;
            (d, d)
        }
        // Divided rectangle: width = text_w + padding(15); the box height is the
        // label box (bbox.h + padding) grown by 20% for the divider strip.
        NodeShape::DividedRect => (text_w + 15.0, 1.2 * (34.0 + extra)),
        // Lined/shaded process: rect + a left frame bar. width = text_w + 2·pad +
        // 2·FRAME_WIDTH(8); height = bbox.h + 2·pad.
        NodeShape::LinedProcess => (text_w + 46.0, NODE_HEIGHT + extra),
        // Window pane / stacked rect: width/height = bbox + 2·pad + rectOffset(10).
        NodeShape::WindowPane | NodeShape::StackedRect => (text_w + 40.0, 59.0 + extra),
        // Notched rectangle (card): width = bbox + pad(15) + NOTCH_SIZE(12);
        // height = bbox.h + pad. Exact `<polygon>`.
        NodeShape::NotchedRect => (text_w + 27.0, POLY_H + extra),
        // Notched pentagon: width/height = bbox + 2·pad(15).
        NodeShape::NotchedPentagon => (text_w + 30.0, 49.0 + extra),
        // Triangle / flipped triangle: mermaid w = bbox + pad(15); h = w + bbox.h.
        // The drawn triangle base (tw) equals h, so the visual box is h×h.
        NodeShape::Triangle | NodeShape::FlippedTriangle => {
            let h = text_w + 15.0 + 19.0 + extra;
            (h, h)
        }
        // Sloped rectangle: width = bbox + 2·pad; the drawn height spans 1.5·h.
        NodeShape::SlopedRect => (text_w + 30.0, 1.5 * (49.0 + extra)),
        // Curved trapezoid (display): width = (bbox + 2·pad)·1.25; height = bbox + 2·pad.
        NodeShape::CurvedTrapezoid => ((text_w + 30.0) * 1.25, 49.0 + extra),
        // Fixed-size markers (no label): junction r=7, stop 14, crossed r=30.
        NodeShape::FilledCircle | NodeShape::FramedCircle => (14.0, 14.0),
        NodeShape::CrossedCircle => (60.0, 60.0),
        // Delay: rect + rounded right end. w/h = bbox + 2·pad.
        NodeShape::Delay => (text_w + 30.0, NODE_HEIGHT + extra),
        // Document family: wavy bottom adds ~25% height. lin/tag add a left line /
        // corner tag width.
        NodeShape::Document => (text_w + 30.0, (NODE_HEIGHT + extra) * 1.25),
        NodeShape::LinedDocument => (text_w + 38.0, (NODE_HEIGHT + extra) * 1.25),
        NodeShape::TaggedDocument => (text_w + 39.0, (NODE_HEIGHT + extra) * 1.25),
        // Stacked documents: extra width/height for the offset stack.
        NodeShape::Documents => (text_w + 50.0, (NODE_HEIGHT + extra) * 1.96),
        // Tagged rect: bbox + 2·pad + tag fold (~TAG_RATIO·h).
        NodeShape::TaggedRect => (text_w + 39.0, NODE_HEIGHT + extra),
        // Bow-tie rect: bbox + 2·pad + arc sagitta; height = bbox + pad.
        NodeShape::BowTieRect => (text_w + 35.0, POLY_H + extra),
        // Wave rectangle (flag): wavy top+bottom grows height ~1.5×.
        NodeShape::WaveRect => (text_w + 30.0, (POLY_H + extra) * 1.5),
        // Text block: borderless rect, bbox + pad.
        NodeShape::TextBlock => (text_w + 15.0, POLY_H + extra),
        // Horizontal cylinder (mermaid `tiltedCylinder`): labelPadding = pad/2 =
        // 7.5. h = bbox.h + 7.5; the end caps (radius rx = ry/(2.5 + h/50) with
        // ry = h/2, i.e. cylinder_ry(h)) add rx to the body plus another rx of
        // overhang, so dagre width = (bbox + 7.5 + rx) + rx = bbox + 7.5 + 3·rx.
        NodeShape::HorizontalCylinder => {
            let h = 19.0 + extra + 7.5;
            let rx = cylinder_ry(h);
            (text_w + 7.5 + 3.0 * rx, h)
        }
        // Lined (disk) cylinder (mermaid `linedCylinder`): w = bbox + 2·pad(30);
        // ry = cylinder_ry(w); h = bbox.h + 2·pad(30) + 3·ry (top + bottom caps).
        NodeShape::LinedCylinder => {
            let w = text_w + 30.0;
            let ry = cylinder_ry(w);
            (w, 19.0 + extra + 30.0 + 3.0 * ry)
        }
        // Fork/join bar: a fixed rectangle whose orientation follows the graph
        // direction (mermaid `forkJoin`): 10×70 in LR, 70×10 otherwise.
        NodeShape::Fork => {
            if matches!(dir, Direction::LeftRight) {
                (10.0, 70.0)
            } else {
                (70.0, 10.0)
            }
        }
        NodeShape::Hourglass => (30.0, 30.0),
        NodeShape::LightningBolt => (35.0, 70.0),
        // Bang (mermaid `bang`): explosion. w = bbox + 10·halfPadding(75),
        // h = bbox.h + 8·halfPadding(60); the drawn arcs overshoot the box by
        // 0.15 each side, so the rendered extent (= dagre size) is 1.25× both.
        NodeShape::Bang => {
            let ew = (text_w + 75.0).max(text_w + 20.0);
            let eh = (19.0 + extra + 60.0).max(19.0 + extra + 20.0);
            (1.25 * ew, 1.25 * eh)
        }
        // Cloud (mermaid `cloud`): w = bbox + pad(15), h = bbox.h + pad(15). The
        // rounded lobes bulge out non-linearly; a linear fit of the rendered
        // extent (valid for single-line labels) reproduces mermaid's box.
        NodeShape::Cloud => {
            let w = text_w + 15.0;
            (0.747 * w + 43.33, 0.4806 * w + (19.0 + extra + 15.0))
        }
        // Curly braces (mermaid `curlyBrace*`): stroke-only brace(s) around a
        // body of bbox + pad(15); the brace arcs add ~2·radius (10) of extent
        // for a single brace, ~2.5·radius (12.5) for both. Height = body + 2·r.
        NodeShape::BraceLeft | NodeShape::BraceRight => (text_w + 25.0, 44.0 + extra),
        NodeShape::Braces => (text_w + 27.5, 44.0 + extra),
        // Stadium (mermaid `stadium`): a pill. h = bbox.h + pad(15) = 34;
        // w = bbox + h/4 + pad(15). The semicircular caps stay within w.
        NodeShape::Stadium => {
            let h = POLY_H + extra;
            (text_w + h / 4.0 + 15.0, h)
        }
        // Cylinder (`[(db)]`): a 3D database shape. mermaid sizes it
        // w = text + padding(15); ry = (w/2)/(2.5 + w/50); the drawn height is
        // the label bbox (19 for one line, +17.6 per extra) plus padding(15)
        // plus 3·ry (the top and bottom ellipse caps). See render::render_shape.
        NodeShape::Cylinder => {
            let w = text_w + 15.0;
            let ry = cylinder_ry(w);
            (w, 19.0 + extra + 15.0 + 3.0 * ry)
        }
    }
}

/// The elliptical cap radius `ry` of a cylinder of total width `w`, matching
/// mermaid's `rx / (2.5 + w/50)` with `rx = w/2`.
pub fn cylinder_ry(w: f64) -> f64 {
    (w / 2.0) / (2.5 + w / 50.0)
}

fn rank_dir(dir: Direction) -> RankDir {
    match dir {
        Direction::TopBottom => RankDir::TB,
        Direction::BottomTop => RankDir::BT,
        Direction::LeftRight => RankDir::LR,
        Direction::RightLeft => RankDir::RL,
    }
}

// --- Subgraph direction (mermaid's separately-laid-out subgraphs). ---
// mermaid lays a subgraph out on its own — with a transposed/explicit direction
// — only when no edge crosses its boundary ("no external connections"); those
// get `ranksep += 25` and a padded cluster box. Subgraphs an edge enters/leaves
// stay inline in the parent's compound layout (the default path below), which
// already matches mermaid. This mirrors mermaid's dagre-wrapper `extractor` +
// `recursiveRender`.
const SUBGRAPH_RANKSEP_INC: f64 = 25.0;
/// Direction a separately-laid-out subgraph uses: explicit, else transposed
/// from the parent (`TB -> LR`, anything else -> `TB`).
fn subgraph_dir(sg: &crate::ir::Subgraph, parent: Direction) -> Direction {
    sg.direction
        .unwrap_or(if matches!(parent, Direction::TopBottom) {
            Direction::LeftRight
        } else {
            Direction::TopBottom
        })
}

/// The subgraphs containing `node`, innermost first.
fn node_ancestors(chart: &Flowchart, n: usize) -> Vec<usize> {
    let mut v = Vec::new();
    let mut cur = chart.nodes[n].subgraph;
    while let Some(sg) = cur {
        v.push(sg);
        cur = chart.subgraphs[sg].parent;
    }
    v
}

/// Which subgraphs are laid out separately: those with no edge having exactly
/// one endpoint as an interior descendant node (no boundary-crossing edge).
fn compute_extracted(chart: &Flowchart) -> Vec<bool> {
    let mut ext = vec![true; chart.subgraphs.len()];
    for e in &chart.edges {
        let (fa, ta) = (node_ancestors(chart, e.from), node_ancestors(chart, e.to));
        for (sg, ext) in ext.iter_mut().enumerate() {
            if fa.contains(&sg) != ta.contains(&sg) {
                *ext = false;
            }
        }
    }
    ext
}

/// Deepest extracted subgraph containing node `n` (its layout "home"), or `None`.
fn home_node(chart: &Flowchart, ext: &[bool], n: usize) -> Option<usize> {
    node_ancestors(chart, n).into_iter().find(|&sg| ext[sg])
}

/// The scope (owner) an edge endpoint is placed in. For a subgraph-reference
/// endpoint (an edge naming a subgraph) that is the scope holding its collapsed
/// cluster node; otherwise the node's own home scope.
fn endpoint_home(chart: &Flowchart, ext: &[bool], n: usize) -> Option<usize> {
    match chart.nodes[n].subgraph_ref {
        Some(s) => home_sg(chart, ext, s),
        None => home_node(chart, ext, n),
    }
}

/// The dagre node key an edge endpoint attaches to: the collapsed `s{sg}` node
/// for an extracted subgraph reference, the `c{sg}` cluster for a non-extracted
/// one, else the ordinary `n{node}`.
fn endpoint_key(chart: &Flowchart, ext: &[bool], n: usize) -> String {
    match chart.nodes[n].subgraph_ref {
        Some(s) if ext[s] => format!("s{s}"),
        Some(s) => format!("c{s}"),
        None => format!("n{n}"),
    }
}

/// mermaid's cluster render order: a DFS of the subgraph tree visiting siblings
/// in reverse definition order, parents before children. Returns a rank per
/// subgraph (lower renders first).
fn render_order(chart: &Flowchart) -> Vec<usize> {
    fn dfs(chart: &Flowchart, parent: Option<usize>, rank: &mut Vec<usize>, next: &mut usize) {
        let mut children: Vec<usize> = (0..chart.subgraphs.len())
            .filter(|&s| chart.subgraphs[s].parent == parent)
            .collect();
        children.reverse();
        for c in children {
            rank[c] = *next;
            *next += 1;
            dfs(chart, Some(c), rank, next);
        }
    }
    let mut rank = vec![0usize; chart.subgraphs.len()];
    let mut next = 0;
    dfs(chart, None, &mut rank, &mut next);
    rank
}

/// Deepest extracted subgraph strictly containing subgraph `s`, or `None`.
fn home_sg(chart: &Flowchart, ext: &[bool], s: usize) -> Option<usize> {
    let mut cur = chart.subgraphs[s].parent;
    while let Some(sg) = cur {
        if ext[sg] {
            return Some(sg);
        }
        cur = chart.subgraphs[sg].parent;
    }
    None
}

/// Geometry of one layout scope (the root or a separately-laid-out subgraph), in
/// its own dagre frame. For an extracted subgraph, `own_*` is its cluster box
/// (from the compound layout) — the size/centre a parent uses to place it.
#[derive(Default)]
struct Scope {
    nodes: Vec<(usize, f64, f64)>,
    clusters: Vec<(usize, f64, f64, f64, f64)>,
    edges: Vec<(usize, Vec<(f64, f64)>)>,
    own_cx: f64,
    own_cy: f64,
    own_w: f64,
    own_h: f64,
}

/// Lay out one scope with dagre. An extracted subgraph is laid out as a compound
/// graph *with itself as the enclosing cluster* (mermaid re-inserts the subgraph
/// into its own sub-graph), so dagre yields the exact cluster box and the +25
/// ranksep border spacing. Inline (non-extracted) child subgraphs are ordinary
/// compound clusters; extracted child subgraphs are collapsed to a node sized to
/// their own cluster box, then their contents merged back in at that position.
/// `offsets[s]` records each extracted child's placement offset in this frame.
fn layout_scope(
    chart: &Flowchart,
    sizes: &[(f64, f64)],
    ext: &[bool],
    owner: Option<usize>,
    dir: Direction,
    ranksep: f64,
    offsets: &mut Vec<(f64, f64)>,
) -> Scope {
    use std::collections::HashMap;
    // Recursively lay out extracted child subgraphs; their own cluster box is the
    // collapsed-node size in this scope.
    let mut sub: HashMap<usize, Scope> = HashMap::new();
    for s in 0..chart.subgraphs.len() {
        if home_sg(chart, ext, s) == owner && ext[s] {
            let sdir = subgraph_dir(&chart.subgraphs[s], dir);
            let sc = layout_scope(
                chart,
                sizes,
                ext,
                Some(s),
                sdir,
                ranksep + SUBGRAPH_RANKSEP_INC,
                offsets,
            );
            sub.insert(s, sc);
        }
    }

    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });

    // An extracted subgraph is its own enclosing cluster in this layout.
    if let Some(o) = owner {
        g.set_node(format!("c{o}"), Some(NodeLabel::default()));
    }
    for (n, node) in chart.nodes.iter().enumerate() {
        if node.subgraph_ref.is_some() {
            continue; // stands in for a subgraph; the cluster node carries it
        }
        if home_node(chart, ext, n) == owner {
            let (w, h) = sizes[n];
            g.set_node(
                format!("n{n}"),
                Some(NodeLabel {
                    width: w,
                    height: h,
                    ..Default::default()
                }),
            );
        }
    }
    for s in 0..chart.subgraphs.len() {
        if home_sg(chart, ext, s) != owner {
            continue;
        }
        if ext[s] {
            let c = &sub[&s];
            g.set_node(
                format!("s{s}"),
                Some(NodeLabel {
                    width: c.own_w,
                    height: c.own_h,
                    ..Default::default()
                }),
            );
        } else {
            g.set_node(format!("c{s}"), Some(NodeLabel::default()));
        }
    }
    // Compound parent of a member: its enclosing subgraph when that is this
    // scope's own cluster, or an inline sub-cluster of this scope.
    let parent_key = |parent: Option<usize>| match parent {
        Some(p) if Some(p) == owner => Some(format!("c{p}")),
        Some(p) if home_sg(chart, ext, p) == owner && !ext[p] => Some(format!("c{p}")),
        _ => None,
    };
    for n in 0..chart.nodes.len() {
        if chart.nodes[n].subgraph_ref.is_some() {
            continue;
        }
        if home_node(chart, ext, n) == owner {
            if let Some(pk) = parent_key(chart.nodes[n].subgraph) {
                g.set_parent(&format!("n{n}"), Some(&pk));
            }
        }
    }
    for s in 0..chart.subgraphs.len() {
        if home_sg(chart, ext, s) != owner {
            continue;
        }
        let key = if ext[s] {
            format!("s{s}")
        } else {
            format!("c{s}")
        };
        if let Some(pk) = parent_key(chart.subgraphs[s].parent) {
            g.set_parent(&key, Some(&pk));
        }
    }
    for (i, e) in chart.edges.iter().enumerate() {
        if endpoint_home(chart, ext, e.from) == owner && endpoint_home(chart, ext, e.to) == owner {
            let (lw, lh) = match &e.label {
                Some(l) => (
                    crate::text::measure_width(l, FONT_SIZE),
                    crate::text::line_height(FONT_SIZE),
                ),
                None => (0.0, 0.0),
            };
            g.set_edge(
                endpoint_key(chart, ext, e.from),
                endpoint_key(chart, ext, e.to),
                Some(EdgeLabel {
                    width: lw,
                    height: lh,
                    minlen: e.min_len.max(1) as i32,
                    ..Default::default()
                }),
                Some(i.to_string().as_str()),
            );
        }
    }

    dagre_layout(
        &mut g,
        Some(LayoutOptions {
            rankdir: rank_dir(dir),
            nodesep: chart.node_spacing.unwrap_or(NODE_SEP),
            ranksep,
            edgesep: EDGE_SEP,
            marginx: MARGIN,
            marginy: MARGIN,
            tie_keep_first: true,
            ..Default::default()
        }),
    );

    let mut out = Scope::default();
    for n in 0..chart.nodes.len() {
        if chart.nodes[n].subgraph_ref.is_some() {
            continue;
        }
        if home_node(chart, ext, n) == owner {
            let node = g.node(&format!("n{n}"));
            out.nodes.push((
                n,
                node.and_then(|x| x.x).unwrap_or(0.0),
                node.and_then(|x| x.y).unwrap_or(0.0),
            ));
        }
    }
    for s in 0..chart.subgraphs.len() {
        if home_sg(chart, ext, s) != owner {
            continue;
        }
        if ext[s] {
            let node = g.node(&format!("s{s}"));
            let px = node.and_then(|x| x.x).unwrap_or(0.0);
            let py = node.and_then(|x| x.y).unwrap_or(0.0);
            // Merge the child's contents in this frame, aligning its own cluster
            // centre to where dagre placed the collapsed node.
            let c = &sub[&s];
            let (ox, oy) = (px - c.own_cx, py - c.own_cy);
            offsets[s] = (ox, oy);
            for &(ni, x, y) in &c.nodes {
                out.nodes.push((ni, x + ox, y + oy));
            }
            for &(s2, cx, cy, w, h) in &c.clusters {
                out.clusters.push((s2, cx + ox, cy + oy, w, h));
            }
            for (e2, pts) in &c.edges {
                out.edges
                    .push((*e2, pts.iter().map(|&(x, y)| (x + ox, y + oy)).collect()));
            }
        } else {
            let node = g.node(&format!("c{s}"));
            out.clusters.push((
                s,
                node.and_then(|x| x.x).unwrap_or(0.0),
                node.and_then(|x| x.y).unwrap_or(0.0),
                node.map(|x| x.width).unwrap_or(0.0),
                node.map(|x| x.height).unwrap_or(0.0),
            ));
        }
    }
    for (i, e) in chart.edges.iter().enumerate() {
        if endpoint_home(chart, ext, e.from) == owner && endpoint_home(chart, ext, e.to) == owner {
            let (fk, tk) = (
                endpoint_key(chart, ext, e.from),
                endpoint_key(chart, ext, e.to),
            );
            if let Some(el) = g.edge(&fk, &tk, Some(&i.to_string())) {
                let pts: Vec<(f64, f64)> = el.points.iter().map(|p| (p.x, p.y)).collect();
                if !pts.is_empty() {
                    out.edges.push((i, pts));
                }
            }
        }
    }

    // For an extracted subgraph, record its own cluster box (dagre gives it
    // directly) and emit its rect.
    if let Some(o) = owner {
        let node = g.node(&format!("c{o}"));
        out.own_cx = node.and_then(|x| x.x).unwrap_or(0.0);
        out.own_cy = node.and_then(|x| x.y).unwrap_or(0.0);
        out.own_w = node.map(|x| x.width).unwrap_or(0.0);
        out.own_h = node.map(|x| x.height).unwrap_or(0.0);
        out.clusters
            .push((o, out.own_cx, out.own_cy, out.own_w, out.own_h));
    }
    out
}

fn layout_flowchart(chart: &Flowchart) -> LaidOutFlowchart {
    // Our node sizes (used both as dagre input and as the rendered box sizes).
    let sizes: Vec<(f64, f64)> = chart
        .nodes
        .iter()
        .map(|n| node_size(&n.label, n.shape, chart.direction))
        .collect();

    if chart.nodes.is_empty() {
        return LaidOutFlowchart {
            direction: chart.direction,
            origin_x: 0.0,
            origin_y: 0.0,
            width: MARGIN * 2.0,
            height: MARGIN * 2.0,
            nodes: Vec::new(),
            edges: Vec::new(),
            clusters: Vec::new(),
            look: chart.look,
            theme: chart.theme,
            title: chart.title.clone(),
            acc_title: chart.acc_title.clone(),
            acc_descr: chart.acc_descr.clone(),
            scope_offsets: Vec::new(),
        };
    }

    let ext = compute_extracted(chart);
    let order = render_order(chart);
    // Per-subgraph render offset (relative to the parent scope), filled in by
    // layout, then composed to absolute offsets below.
    let mut offsets = vec![(0.0, 0.0); chart.subgraphs.len()];
    let scope = layout_scope(
        chart,
        &sizes,
        &ext,
        None,
        chart.direction,
        chart.rank_spacing.unwrap_or(RANK_SEP),
        &mut offsets,
    );
    // Compose parent-relative offsets into absolute scope offsets: a subgraph's
    // absolute offset is its own relative offset plus its parent scope's.
    let mut scope_offsets = vec![(0.0, 0.0); chart.subgraphs.len()];
    for s in 0..chart.subgraphs.len() {
        let mut acc = offsets[s];
        let mut cur = home_sg(chart, &ext, s);
        while let Some(p) = cur {
            acc.0 += offsets[p].0;
            acc.1 += offsets[p].1;
            cur = home_sg(chart, &ext, p);
        }
        scope_offsets[s] = acc;
    }

    let nodes: Vec<PlacedNode> = scope
        .nodes
        .iter()
        .map(|&(i, cx, cy)| {
            let (w, h) = sizes[i];
            let n = &chart.nodes[i];
            let st = crate::style::resolve(&chart.class_defs, &n.classes, &n.styles, true);
            PlacedNode {
                id: n.id.clone(),
                label: n.label.clone(),
                shape: n.shape,
                cx,
                cy,
                width: w,
                height: h,
                home: home_node(chart, &ext, i),
                shape_style: st.shape,
                label_style: st.label,
                classes: n.classes.clone(),
            }
        })
        .collect();
    // PlacedEdge endpoints index into `nodes`, whose order follows scope traversal.
    let mut node_at = vec![0usize; chart.nodes.len()];
    for (pi, &(i, ..)) in scope.nodes.iter().enumerate() {
        node_at[i] = pi;
    }

    let edges: Vec<PlacedEdge> = scope
        .edges
        .iter()
        .map(|(i, points)| {
            let e = &chart.edges[*i];
            let label_pos = e.label.as_ref().map(|_| midpoint_by_length(points));
            // Resolve linkStyle. The effective declarations are the global
            // `linkStyle default` decls followed, for an edge with its own
            // per-index linkStyle, by those decls plus `fill:none` (mermaid only
            // adds fill:none to an edge's own style, not to the default). The
            // last `stroke:`/`color:` win — stroke selects a colour-matched arrow
            // marker, color paints the edge label.
            let mut decls = chart.link_style_default.clone();
            if !e.link_style.is_empty() {
                decls.extend(e.link_style.iter().cloned());
                decls.push("fill:none".into());
            }
            let stroke = decls
                .iter()
                .rev()
                .find_map(|d| d.strip_prefix("stroke:").map(|v| v.trim().to_string()));
            let label_color = decls
                .iter()
                .rev()
                .find_map(|d| d.strip_prefix("color:").map(|v| v.trim().to_string()));
            let style = decls.join(";");
            let from_cluster = chart.nodes[e.from].subgraph_ref.is_some();
            let to_cluster = chart.nodes[e.to].subgraph_ref.is_some();
            PlacedEdge {
                from: if from_cluster { 0 } else { node_at[e.from] },
                to: if to_cluster { 0 } else { node_at[e.to] },
                from_id: chart.nodes[e.from].id.clone(),
                to_id: chart.nodes[e.to].id.clone(),
                from_cluster,
                to_cluster,
                label: e.label.clone(),
                points: points.clone(),
                arrow_start: e.arrow_start,
                arrow_end: e.arrow_end,
                label_pos,
                style,
                stroke,
                label_color,
                home: endpoint_home(chart, &ext, e.from),
            }
        })
        .collect();

    let clusters: Vec<PlacedCluster> = scope
        .clusters
        .iter()
        .map(|&(s, cx, cy, w, h)| {
            let sg = &chart.subgraphs[s];
            let st = crate::style::resolve(&chart.class_defs, &sg.classes, &sg.styles, false);
            PlacedCluster {
                id: sg.id.clone(),
                title: sg.title.clone(),
                cx,
                cy,
                width: w,
                height: h,
                sg_index: s,
                extracted: ext[s],
                home: home_sg(chart, &ext, s),
                order: order[s],
                shape_style: st.shape,
                classes: sg.classes.clone(),
            }
        })
        .collect();

    // Content bounding box = union of node boxes, cluster boxes, and every edge
    // waypoint. Edges (curved back-edges especially) can extend beyond the node
    // band, so the viewBox must include them or they get clipped — this is what
    // mermaid's getBBox-based viewBox captures.
    let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
    let (mut max_x, mut max_y) = (f64::MIN, f64::MIN);
    let mut expand = |x: f64, y: f64| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };
    for n in &nodes {
        expand(n.cx - n.width / 2.0, n.cy - n.height / 2.0);
        expand(n.cx + n.width / 2.0, n.cy + n.height / 2.0);
    }
    for c in &clusters {
        expand(c.cx - c.width / 2.0, c.cy - c.height / 2.0);
        expand(c.cx + c.width / 2.0, c.cy + c.height / 2.0);
    }
    for e in &edges {
        for &(x, y) in &e.points {
            expand(x, y);
        }
    }
    let origin_x = min_x - MARGIN;
    let origin_y = min_y - MARGIN;
    let width = (max_x - min_x) + 2.0 * MARGIN;
    let height = (max_y - min_y) + 2.0 * MARGIN;

    LaidOutFlowchart {
        direction: chart.direction,
        origin_x,
        origin_y,
        width,
        height,
        nodes,
        edges,
        clusters,
        look: chart.look,
        theme: chart.theme,
        title: chart.title.clone(),
        acc_title: chart.acc_title.clone(),
        acc_descr: chart.acc_descr.clone(),
        scope_offsets,
    }
}

/// The point at 50% of a polyline's arc length — where mermaid places edge labels.
fn midpoint_by_length(points: &[(f64, f64)]) -> (f64, f64) {
    match points {
        [] => (0.0, 0.0),
        [p] => *p,
        _ => {
            let seg =
                |a: (f64, f64), b: (f64, f64)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            let total: f64 = points.windows(2).map(|w| seg(w[0], w[1])).sum();
            let mut remaining = total / 2.0;
            for w in points.windows(2) {
                let d = seg(w[0], w[1]);
                if d >= remaining {
                    let t = if d > 0.0 { remaining / d } else { 0.0 };
                    return (
                        w[0].0 + (w[1].0 - w[0].0) * t,
                        w[0].1 + (w[1].1 - w[0].1) * t,
                    );
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
            _ => unreachable!("expected a flowchart"),
        }
    }

    #[test]
    fn rhombus_and_circle_are_square() {
        let f = flowchart("flowchart TD\n A{Ready} --> B((Go))");
        for n in &f.nodes {
            assert!(
                (n.width - n.height).abs() < 1e-9,
                "{} not square: {}x{}",
                n.id,
                n.width,
                n.height
            );
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
        assert!(
            approx(f.width, 128.86) && approx(f.height, 263.0),
            "{}x{}",
            f.width,
            f.height
        );
    }
}
