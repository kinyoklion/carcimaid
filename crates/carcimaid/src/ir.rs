//! The intermediate representation: a layout-independent description of a
//! parsed diagram. Parsing produces an [`Diagram`]; layout consumes it.

/// A parsed mermaid diagram. One variant per supported diagram type; new
/// diagram types are added as variants so the layout/render stages can
/// exhaustively dispatch on them.
#[derive(Debug, Clone, PartialEq)]
pub enum Diagram {
    /// `flowchart` / `graph` diagrams.
    Flowchart(Flowchart),
}

/// Flow direction, mirroring mermaid's `TD`/`TB`/`BT`/`LR`/`RL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// Top to bottom (mermaid `TD`/`TB`). The mermaid default.
    #[default]
    TopBottom,
    /// Bottom to top (`BT`).
    BottomTop,
    /// Left to right (`LR`).
    LeftRight,
    /// Right to left (`RL`).
    RightLeft,
}

/// A flowchart: a directed graph of [`Node`]s connected by [`Edge`]s, with
/// optional [`Subgraph`] groupings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Flowchart {
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub subgraphs: Vec<Subgraph>,
}

impl Flowchart {
    /// Index of the node with the given id, if present.
    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }
}

/// A `subgraph … end` grouping (rendered as a dagre cluster). Membership is
/// recorded on each [`Node`] via [`Node::subgraph`]; nesting via [`Subgraph::parent`].
#[derive(Debug, Clone, PartialEq)]
pub struct Subgraph {
    /// Identifier (from `subgraph id[title]`, else derived from the title).
    pub id: String,
    /// Display title.
    pub title: String,
    /// Index into [`Flowchart::subgraphs`] of the enclosing subgraph, if nested.
    pub parent: Option<usize>,
}

/// The outline shape of a flowchart node, selected by mermaid's bracket syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeShape {
    /// `A[text]` — rectangle. The default when an id appears bare.
    #[default]
    Rectangle,
    /// `A(text)` — rounded rectangle.
    RoundedRectangle,
    /// `A{text}` — rhombus / decision.
    Rhombus,
    /// `A((text))` — circle.
    Circle,
    /// `A([text])` — stadium / pill.
    Stadium,
}

/// A flowchart node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The identifier used in the source (e.g. `A`).
    pub id: String,
    /// Display text. Defaults to the id when no label is given.
    pub label: String,
    pub shape: NodeShape,
    /// Index into [`Flowchart::subgraphs`] of the subgraph this node belongs to,
    /// if any (the subgraph in whose block the node first appeared).
    pub subgraph: Option<usize>,
}

/// The line style of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeStyle {
    /// `-->` / `---` solid line.
    #[default]
    Solid,
    /// `-.->` dotted line.
    Dotted,
    /// `==>` thick line.
    Thick,
}

/// A directed (or plain) connection between two nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Index into [`Flowchart::nodes`].
    pub from: usize,
    /// Index into [`Flowchart::nodes`].
    pub to: usize,
    /// Optional text label on the edge (`A -->|label| B`).
    pub label: Option<String>,
    pub style: EdgeStyle,
    /// Whether the edge draws an arrowhead at the `to` end.
    pub arrow: bool,
}
