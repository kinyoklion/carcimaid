//! The intermediate representation: a layout-independent description of a
//! parsed diagram. Parsing produces an [`Diagram`]; layout consumes it.

/// A parsed mermaid diagram. One variant per supported diagram type; new
/// diagram types are added as variants so the layout/render stages can
/// exhaustively dispatch on them.
#[derive(Debug, Clone, PartialEq)]
pub enum Diagram {
    /// `flowchart` / `graph` diagrams.
    Flowchart(Flowchart),
    /// `sequenceDiagram` diagrams.
    Sequence(SequenceDiagram),
}

impl Diagram {
    /// Apply a fallback colour [`Theme`] chosen by the caller. This is a
    /// *default*, not an override: callers are expected to invoke it only when
    /// the source did not select a theme itself (see
    /// [`crate::render_to_svg_with`]).
    ///
    /// Honoured for flowcharts and sequence diagrams.
    pub fn set_default_theme(&mut self, theme: Theme) {
        match self {
            Diagram::Flowchart(f) => f.theme = theme,
            Diagram::Sequence(s) => s.theme = theme,
        }
    }
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

/// The visual "look" of a diagram (mermaid frontmatter `look:`). Selects the
/// rough.js roughness used when rendering node shapes through `roughr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Look {
    /// mermaid's default clean look — rough.js roughness `0` (deterministic
    /// fill path = the exact shape vertices).
    #[default]
    Classic,
    /// The hand-drawn look — rough.js roughness `0.7`.
    HandDrawn,
}

impl Look {
    /// The rough.js `roughness` option for this look.
    pub fn roughness(self) -> f64 {
        match self {
            Look::Classic => 0.0,
            Look::HandDrawn => 0.7,
        }
    }
}

/// The colour theme of a diagram (mermaid frontmatter `config.theme`). Selects
/// the [`Palette`] the renderer paints node/cluster/edge colours from. Defaults
/// to [`Theme::Default`] (mermaid's built-in default theme).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// mermaid's built-in default theme (purple nodes on white).
    #[default]
    Default,
    /// `theme: base` — the light beige/amber base theme.
    Base,
    /// `theme: forest` — greens.
    Forest,
    /// `theme: dark` — light-on-dark greys.
    Dark,
    /// `theme: neutral` — greyscale.
    Neutral,
}

/// The concrete colours a theme paints. The renderer pulls every default node,
/// cluster, edge, marker and text colour from here so a diagram's `config.theme`
/// selects mermaid's matching palette. Inline `classDef`/`style` colours still
/// win (they are emitted as inline `style` attributes, which override these
/// theme defaults) — only the *defaults* change per theme.
///
/// Values are the exact strings mermaid emits, probed from the mermaid CLI per
/// theme (`node fill/stroke`, `cluster fill/stroke`, `lineColor`, node text
/// colour and edge-label background). `line_color` is the form used in shape
/// presentation attributes (e.g. `#333333`); `line_color_css` is the (possibly
/// abbreviated) form mermaid writes into the `<style>` block (e.g. `#333`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Node shape fill (mermaid `mainBkg`/`nodeBkg`).
    pub node_bkg: &'static str,
    /// Node shape border (mermaid `nodeBorder`).
    pub node_border: &'static str,
    /// Cluster (subgraph) rect fill.
    pub cluster_bkg: &'static str,
    /// Cluster (subgraph) rect border.
    pub cluster_border: &'static str,
    /// Line/marker/fork colour, in shape presentation-attribute form.
    pub line_color: &'static str,
    /// Line/marker colour as mermaid writes it into the `<style>` block.
    pub line_color_css: &'static str,
    /// Node/label text colour (and the root `fill`), for the `<style>` block.
    pub text_color: &'static str,
    /// Edge-label background colour, for the `<style>` block.
    pub edge_label_bg: &'static str,
    /// The two `<stop>` colours of the `<linearGradient id="…-gradient">` that
    /// mermaid appends to the SVG for every *non-default* theme (used by the neo
    /// look's gradient stroke). `None` for the default theme, which emits no
    /// gradient element — matching mermaid and keeping default output identical.
    pub gradient_stops: Option<(&'static str, &'static str)>,
}

impl Theme {
    /// The [`Palette`] for this theme (colours probed from the mermaid CLI).
    pub fn palette(self) -> Palette {
        match self {
            // Default palette = the values carcimaid hardcoded before theme
            // support, so default-theme output stays byte-identical.
            Theme::Default => Palette {
                node_bkg: "#ECECFF",
                node_border: "#9370DB",
                cluster_bkg: "#ffffde",
                cluster_border: "#aaaa33",
                line_color: "#333333",
                line_color_css: "#333",
                text_color: "#333",
                edge_label_bg: "rgba(232,232,232,0.8)",
                gradient_stops: None,
            },
            Theme::Base => Palette {
                node_bkg: "#fff4dd",
                node_border: "hsl(40.5882352941, 60%, 83.3333333333%)",
                cluster_bkg: "hsl(220.5882352941, 100%, 98.3333333333%)",
                cluster_border: "hsl(220.5882352941, 60%, 88.3333333333%)",
                line_color: "#0b0b0b",
                line_color_css: "#0b0b0b",
                text_color: "#333",
                edge_label_bg: "hsl(-79.4117647059, 100%, 93.3333333333%)",
                gradient_stops: Some((
                    "hsl(40.5882352941, 60%, 83.3333333333%)",
                    "hsl(-79.4117647059, 60%, 83.3333333333%)",
                )),
            },
            Theme::Forest => Palette {
                node_bkg: "#cde498",
                node_border: "#13540c",
                cluster_bkg: "#cdffb2",
                cluster_border: "#6eaa49",
                line_color: "#000000",
                line_color_css: "#000000",
                text_color: "#000000",
                edge_label_bg: "#e8e8e8",
                gradient_stops: Some((
                    "hsl(78.1578947368, 18.4615384615%, 64.5098039216%)",
                    "hsl(98.961038961, 60%, 74.9019607843%)",
                )),
            },
            Theme::Dark => Palette {
                node_bkg: "#1f2020",
                node_border: "#ccc",
                cluster_bkg: "hsl(180, 1.5873015873%, 28.3529411765%)",
                cluster_border: "rgba(255, 255, 255, 0.25)",
                line_color: "lightgrey",
                line_color_css: "lightgrey",
                text_color: "#ccc",
                edge_label_bg: "hsl(0, 0%, 34.4117647059%)",
                gradient_stops: Some(("#cccccc", "hsl(180, 0%, 18.3529411765%)")),
            },
            Theme::Neutral => Palette {
                node_bkg: "#eee",
                node_border: "#999",
                cluster_bkg: "hsl(0, 0%, 98.9215686275%)",
                cluster_border: "#707070",
                line_color: "#666",
                line_color_css: "#666",
                text_color: "#000000",
                edge_label_bg: "white",
                gradient_stops: Some(("hsl(0, 0%, 83.3333333333%)", "hsl(0, 0%, 88.9215686275%)")),
            },
        }
    }

    /// The [`SeqPalette`] for this theme (sequence-diagram colours probed from
    /// the mermaid CLI). Distinct from [`Theme::palette`] because mermaid's
    /// sequence renderer draws from a different set of theme variables.
    pub fn seq_palette(self) -> SeqPalette {
        match self {
            // Default = the values the sequence `<style>` hardcoded before theme
            // support, so default-theme output stays byte-identical.
            Theme::Default => SeqPalette {
                text_color: "#333",
                actor_border: "#9370DB",
                actor_bkg: "#ECECFF",
                content_text: "black",
                signal_color: "#333",
                label_border: "#9370DB",
                note_border: "#aaaa33",
                note_bkg: "#fff5ad",
                note_text: "black",
                activation_bkg: "#f4f4f4",
                activation_border: "#666",
                seq_num_fill: "white",
            },
            Theme::Base => SeqPalette {
                text_color: "#333",
                actor_border: "hsl(40.5882352941, 60%, 83.3333333333%)",
                actor_bkg: "#fff4dd",
                content_text: "#333",
                signal_color: "#333",
                label_border: "hsl(40.5882352941, 60%, 83.3333333333%)",
                note_border: "hsl(52.6829268293, 60%, 73.9215686275%)",
                note_bkg: "#fff5ad",
                note_text: "#333",
                activation_bkg: "hsl(-79.4117647059, 100%, 93.3333333333%)",
                activation_border: "hsl(-79.4117647059, 100%, 83.3333333333%)",
                seq_num_fill: "#f4f4f4",
            },
            Theme::Forest => SeqPalette {
                text_color: "#000000",
                actor_border: "hsl(78.1578947368, 58.4615384615%, 54.5098039216%)",
                actor_bkg: "#cde498",
                content_text: "black",
                signal_color: "#333",
                label_border: "#326932",
                note_border: "#6eaa49",
                note_bkg: "#fff5ad",
                note_text: "black",
                activation_bkg: "#f4f4f4",
                activation_border: "#666",
                seq_num_fill: "white",
            },
            Theme::Dark => SeqPalette {
                text_color: "#ccc",
                actor_border: "#ccc",
                actor_bkg: "#1f2020",
                content_text: "lightgrey",
                signal_color: "lightgrey",
                label_border: "#ccc",
                note_border: "hsl(180, 0%, 18.3529411765%)",
                note_bkg: "hsl(180, 1.5873015873%, 28.3529411765%)",
                note_text: "rgb(183.8476190475, 181.5523809523, 181.5523809523)",
                activation_bkg: "hsl(180, 1.5873015873%, 28.3529411765%)",
                activation_border: "#ccc",
                seq_num_fill: "black",
            },
            Theme::Neutral => SeqPalette {
                text_color: "#000000",
                actor_border: "hsl(0, 0%, 83%)",
                actor_bkg: "#eee",
                content_text: "#333",
                signal_color: "#333",
                label_border: "hsl(0, 0%, 83%)",
                note_border: "#999",
                note_bkg: "#666",
                note_text: "#fff",
                activation_bkg: "#f4f4f4",
                activation_border: "#666",
                seq_num_fill: "white",
            },
        }
    }
}

/// The concrete colours a theme paints into a **sequence** diagram's `<style>`
/// block. mermaid's sequence renderer emits fixed presentation attributes on the
/// shapes (e.g. `fill="#eaeaea"` on actor boxes) that its themed stylesheet then
/// overrides — the values here are those stylesheet colours, probed per theme
/// from the mermaid CLI. Because carcimaid mirrors those same presentation
/// attributes (and the structural comparator ignores `<style>` text), changing
/// this palette repaints the diagram visually without affecting compliance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeqPalette {
    /// Root text `fill` (`#my-svg{…;fill:…}`).
    pub text_color: &'static str,
    /// Actor box / lifeline stroke (`actorBorder`, `actorLineColor`).
    pub actor_border: &'static str,
    /// Actor box / label box / actor-glyph fill (`actorBkg`).
    pub actor_bkg: &'static str,
    /// Actor, label, loop and section text `fill`.
    pub content_text: &'static str,
    /// Message lines, arrowheads, crosshead, message text and sequence-number
    /// text (`signalColor`/`signalTextColor`).
    pub signal_color: &'static str,
    /// Label-box border and loop-line stroke/fill (`labelBoxBorderColor`).
    pub label_border: &'static str,
    /// Note border (`noteBorderColor`).
    pub note_border: &'static str,
    /// Note fill (`noteBkgColor`).
    pub note_bkg: &'static str,
    /// Note text `fill` (`noteTextColor`).
    pub note_text: &'static str,
    /// Activation-bar fill (`activationBkgColor`).
    pub activation_bkg: &'static str,
    /// Activation-bar stroke (`activationBorderColor`).
    pub activation_border: &'static str,
    /// Sequence-number circle fill (`sequenceNumberColor`).
    pub seq_num_fill: &'static str,
}

/// A flowchart: a directed graph of [`Node`]s connected by [`Edge`]s, with
/// optional [`Subgraph`] groupings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Flowchart {
    pub direction: Direction,
    /// Visual look (`look:` frontmatter). Selects rough.js roughness; defaults
    /// to [`Look::Classic`]. Parsed from the top-level `config.look` key.
    pub look: Look,
    /// Colour theme (`config.theme` frontmatter). Selects the [`Palette`];
    /// defaults to [`Theme::Default`].
    pub theme: Theme,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub subgraphs: Vec<Subgraph>,
    /// Visible diagram title (from YAML frontmatter `title:`) — rendered as a
    /// `<text class="flowchartTitleText">` above the diagram.
    pub title: Option<String>,
    /// Accessibility title (`accTitle:`) — rendered as `<title>`.
    pub acc_title: Option<String>,
    /// Accessibility description (`accDescr:`/`accDescr { … }`) — as `<desc>`.
    pub acc_descr: Option<String>,
    /// `classDef <name> <props>` definitions: name → CSS declarations (`k:v`).
    pub class_defs: std::collections::HashMap<String, Vec<String>>,
    /// `linkStyle default <props>` declarations — applied to every edge (before
    /// any per-index [`Edge::link_style`]), with no `fill:none` of their own.
    pub link_style_default: Vec<String>,
    /// `config.flowchart.nodeSpacing` from YAML frontmatter → dagre `nodesep`
    /// (space between nodes in the same rank). `None` = mermaid default (50).
    pub node_spacing: Option<f64>,
    /// `config.flowchart.rankSpacing` from YAML frontmatter → dagre `ranksep`
    /// (space between ranks). `None` = mermaid default (50).
    pub rank_spacing: Option<f64>,
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
    /// Explicit `direction` set inside the subgraph. `None` means mermaid picks
    /// one transposed from the parent (only for subgraphs it lays out separately).
    pub direction: Option<Direction>,
    /// Class names applied to the subgraph (`class`/`classDef`).
    pub classes: Vec<String>,
    /// Direct style declarations (`style <id> <props>`), as `k:v` strings.
    pub styles: Vec<String>,
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
    /// `A{{text}}` — hexagon.
    Hexagon,
    /// `A[[text]]` — subroutine (rectangle with side bars).
    Subroutine,
    /// `A[/text/]` — parallelogram leaning right.
    Parallelogram,
    /// `A[\text\]` — parallelogram leaning left.
    LeanLeft,
    /// `A[/text\]` — trapezoid.
    Trapezoid,
    /// `A[\text/]` — inverted trapezoid.
    InvTrapezoid,
    /// `A[(text)]` — cylinder / database (rendered approximately for now).
    Cylinder,
    /// `@{shape: datastore}` — the open-ended data-store symbol: a plain
    /// rectangle drawn with only its top and bottom edges (via a
    /// `stroke-dasharray` of its own width/height). Distinct from [`Cylinder`].
    DataStore,
    /// `A>text]` — the "odd"/flag shape (a rectangle with a notched left edge).
    /// mermaid draws it as a bezier `<path>`; we approximate it as a rectangle.
    Odd,
    /// `@{shape: sm-circ}` — a small fixed-radius (r=7) filled start circle,
    /// rendered without a label.
    SmallCircle,
    /// `@{shape: dbl-circ}` — a double circle (outer + inner, gap 5).
    DoubleCircle,
    /// `@{shape: div-rect}` — a rectangle with a divider line near the top.
    DividedRect,
    /// `@{shape: lin-rect}` — a lined/shaded process (rect with a left bar).
    LinedProcess,
    /// `@{shape: win-pane}` — a window pane (rect split into quadrants).
    WindowPane,
    /// `@{shape: st-rect}` — stacked rectangles (offset outlines behind a rect).
    StackedRect,
    /// `@{shape: notch-rect}` (card) — a rectangle with a notched top-left corner.
    /// Rendered as an exact `<polygon>` (matches mermaid's insertPolygonShape).
    NotchedRect,
    /// `@{shape: notch-pent}` (trapezoidalPentagon) — a pentagon (loop limit).
    NotchedPentagon,
    /// `@{shape: tri}` (triangle) — an upward triangle (extract).
    Triangle,
    /// `@{shape: flip-tri}` (flippedTriangle) — a downward triangle.
    FlippedTriangle,
    /// `@{shape: sl-rect}` (slopedRect) — a rectangle with a sloped top edge.
    SlopedRect,
    /// `@{shape: curv-trap}` (curvedTrapezoid) — a display shape (rounded right).
    CurvedTrapezoid,
    /// `@{shape: f-circ}` (filledCircle) — a small (r=7) solid junction circle.
    FilledCircle,
    /// `@{shape: fr-circ}` (stateEnd) — a framed stop circle (outer + filled inner).
    FramedCircle,
    /// `@{shape: cross-circ}` (crossedCircle) — a circle with an X through it.
    CrossedCircle,
    /// `@{shape: delay}` (halfRoundedRectangle) — rectangle with a rounded right end.
    Delay,
    /// `@{shape: doc}` (waveEdgedRectangle) — a document (wavy bottom edge).
    Document,
    /// `@{shape: docs}` (multiWaveEdgedRectangle) — stacked documents.
    Documents,
    /// `@{shape: lin-doc}` (linedWaveEdgedRect) — a document with a left line.
    LinedDocument,
    /// `@{shape: tag-doc}` (taggedWaveEdgedRectangle) — a document with a corner tag.
    TaggedDocument,
    /// `@{shape: tag-rect}` (taggedRect) — a rectangle with a folded corner tag.
    TaggedRect,
    /// `@{shape: bow-rect}` (bowTieRect) — a rectangle with concave (bow-tie) sides.
    BowTieRect,
    /// `@{shape: flag}` (waveRectangle) — a paper-tape shape (wavy top and bottom).
    WaveRect,
    /// `@{shape: h-cyl}` (tiltedCylinder) — a horizontal cylinder.
    HorizontalCylinder,
    /// `@{shape: lin-cyl}` (linedCylinder) — a lined/disk cylinder.
    LinedCylinder,
    /// `@{shape: fork}` (forkJoin) — a thin filled fork/join bar.
    Fork,
    /// `@{shape: text}` — a borderless text block (rect with class "text").
    TextBlock,
    /// `@{shape: bang}` — an explosion/bang callout.
    Bang,
    /// `@{shape: cloud}` — a cloud.
    Cloud,
    /// `@{shape: hourglass}` (collate) — an hourglass (two triangles).
    Hourglass,
    /// `@{shape: bolt}` (com-link) — a lightning bolt.
    LightningBolt,
    /// `@{shape: brace}` (comment) — a left curly brace.
    BraceLeft,
    /// `@{shape: brace-r}` — a right curly brace.
    BraceRight,
    /// `@{shape: braces}` — curly braces on both sides.
    Braces,
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
    /// Class names applied to the node (`class`/`classDef`/`:::`).
    pub classes: Vec<String>,
    /// Direct style declarations (`style <id> <props>`), as `k:v` strings.
    pub styles: Vec<String>,
    /// When this "node" is actually a reference to a subgraph used as an edge
    /// endpoint (`X --> Y` where `X`/`Y` are subgraph names), the index of that
    /// subgraph. Such nodes are not rendered; the edge attaches to the cluster.
    pub subgraph_ref: Option<usize>,
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

/// An arrowhead type at an edge end (mermaid's `>` / `x` / `o`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrowType {
    /// No arrowhead (open end).
    #[default]
    None,
    /// `>` — the standard triangular arrow (`pointEnd`/`pointStart`).
    Point,
    /// `x` — a cross (`crossEnd`/`crossStart`).
    Cross,
    /// `o` — a circle (`circleEnd`/`circleStart`).
    Circle,
}

/// A directed (or plain) connection between two nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Index into [`Flowchart::nodes`].
    pub from: usize,
    /// Index into [`Flowchart::nodes`].
    pub to: usize,
    /// Optional text label on the edge (`A -->|label| B` or `A -- label --> B`).
    pub label: Option<String>,
    pub style: EdgeStyle,
    /// Arrowhead at the `from` end (`<--`, `x--`, `o--`).
    pub arrow_start: ArrowType,
    /// Arrowhead at the `to` end (`-->`, `--x`, `--o`).
    pub arrow_end: ArrowType,
    /// `linkStyle` CSS declarations (`k:v`) applied to this edge.
    pub link_style: Vec<String>,
    /// Rank distance (dagre `minlen`) this edge spans, derived from the number
    /// of dashes/dots: `-->` is 1, `--->` is 2, `---->` is 3, etc. Extra
    /// dashes make an edge span more ranks, stretching the layout.
    pub min_len: usize,
}

// ---------------------------------------------------------------------------
// Sequence diagrams (`sequenceDiagram`).
// ---------------------------------------------------------------------------

/// A parsed `sequenceDiagram`: an ordered list of [`Participant`]s and a flat,
/// ordered list of [`SeqEvent`]s (messages, notes, block boundaries,
/// activations), mirroring mermaid's linear message model — layout steps
/// vertically through the events in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SequenceDiagram {
    /// Visible title (`title:` line), if any.
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    /// Participants in first-seen (left-to-right) order.
    pub participants: Vec<Participant>,
    /// The ordered event stream.
    pub events: Vec<SeqEvent>,
    /// Participant `box` groupings (a labelled/coloured rect around a run of
    /// participants).
    pub boxes: Vec<SeqBox>,
    /// Colour theme (`config.theme` frontmatter, or the caller's default via
    /// [`crate::render_to_svg_with`]). Selects the [`SeqPalette`]; defaults to
    /// [`Theme::Default`].
    pub theme: Theme,
}

/// A `box … end` participant grouping.
#[derive(Debug, Clone, PartialEq)]
pub struct SeqBox {
    /// Display name (may be empty).
    pub name: String,
    /// Fill colour (CSS), or `None` for a transparent box.
    pub color: Option<String>,
}

impl SequenceDiagram {
    /// Index of the participant with id `id`, if declared.
    pub fn participant_index(&self, id: &str) -> Option<usize> {
        self.participants.iter().position(|p| p.id == id)
    }
}

/// A sequence participant (a lifeline). `actor` participants render as a stick
/// figure; plain `participant`s render as a labelled box.
#[derive(Debug, Clone, PartialEq)]
pub struct Participant {
    /// The identifier used in messages.
    pub id: String,
    /// Display label (the `as` alias, else the id).
    pub label: String,
    /// `true` for the `actor` keyword (stick figure), `false` for `participant`.
    pub is_actor: bool,
    /// Index into [`SequenceDiagram::boxes`] if this participant is inside a
    /// `box … end` grouping.
    pub box_idx: Option<usize>,
    /// `wrap:` directive on the label — wrap it to the actor width (growing the
    /// box height) rather than widening the box to one line.
    pub wrap: bool,
    /// UML shape from `@{ "type": "boundary" }` metadata (boundary/control/
    /// entity/database/collections/queue). `None` = plain box or `actor`.
    pub shape: Option<String>,
}

/// One entry in the sequence's ordered event stream.
#[derive(Debug, Clone, PartialEq)]
pub enum SeqEvent {
    Message(SeqMessage),
    Note(SeqNote),
    /// Turn a participant's activation bar on (`activate X` or `+` on a message).
    Activate(usize),
    /// Turn a participant's activation bar off (`deactivate X` or `-`).
    Deactivate(usize),
    /// A block boundary (loop/alt/opt/par/critical/break/rect). The linear
    /// start/else/end model matches mermaid's LINETYPE stream.
    Block(BlockBoundary),
    /// `autonumber [start [step]]` / `autonumber off` — toggles numbering.
    Autonumber(Option<(i64, i64)>),
    /// `create [participant|actor] X` — the participant's box appears at the
    /// next message rather than the top.
    Create(usize),
    /// `destroy X` — the participant's lifeline ends (with an X) at the next
    /// message referencing it.
    Destroy(usize),
}

/// A message (arrow) between two participants.
#[derive(Debug, Clone, PartialEq)]
pub struct SeqMessage {
    /// Index into [`SequenceDiagram::participants`].
    pub from: usize,
    pub to: usize,
    pub text: String,
    pub arrow: SeqArrow,
    /// `+`/`-` activation suffix: activate the target / deactivate the source.
    pub activate: bool,
    pub deactivate: bool,
    /// `:wrap:` directive — wrap the label to the actor width.
    pub wrap: bool,
}

/// A note attached to one or more participants.
#[derive(Debug, Clone, PartialEq)]
pub struct SeqNote {
    pub placement: NotePlacement,
    /// Participant indices the note spans (one for left/right of, one or two
    /// for `over`).
    pub actors: Vec<usize>,
    pub text: String,
    /// `:wrap:` directive — wrap the text to the note width (vs one line).
    pub wrap: bool,
}

/// Where a note sits relative to its participant(s).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotePlacement {
    LeftOf,
    RightOf,
    Over,
}

/// A block-construct boundary marker in the event stream.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockBoundary {
    LoopStart(String),
    LoopEnd,
    AltStart(String),
    AltElse(String),
    AltEnd,
    OptStart(String),
    OptEnd,
    ParStart(String),
    ParAnd(String),
    ParEnd,
    /// `rect <color>` … `end` — a coloured background region.
    RectStart(String),
    RectEnd,
}

/// The visual style of a sequence message arrow (line pattern + head shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeqArrow {
    /// `->>` — solid line, filled arrowhead.
    SolidArrow,
    /// `-->>` — dotted line, filled arrowhead.
    DottedArrow,
    /// `->` — solid line, open (stick) arrowhead.
    SolidOpen,
    /// `-->` — dotted line, open arrowhead.
    DottedOpen,
    /// `-x` — solid line, cross head.
    SolidCross,
    /// `--x` — dotted line, cross head.
    DottedCross,
    /// `-)` — solid line, open half-arrow (async).
    SolidPoint,
    /// `--)` — dotted line, open half-arrow (async).
    DottedPoint,
    /// `<<->>` — solid line, filled heads both ends (bidirectional).
    BiSolid,
    /// `<<-->>` — dotted line, filled heads both ends (bidirectional).
    BiDotted,
    // Directional (solid-triangle) arrows: `-|\` top, `-|/` bottom, `/|-` and
    // `\|-` their reverse (head at source); `--|…`/`…|--` are the dotted forms.
    SolidTop,
    SolidBottom,
    SolidTopRev,
    SolidBottomRev,
    SolidTopDotted,
    SolidBottomDotted,
    SolidTopRevDotted,
    SolidBottomRevDotted,
    // Stick (open-line head) directional arrows: `-\` top, `-//` bottom, `//-`
    // and `\\-` their reverse; `--…`/`…--` the dotted forms.
    StickTop,
    StickBottom,
    StickTopRev,
    StickBottomRev,
    StickTopDotted,
    StickBottomDotted,
    StickTopRevDotted,
    StickBottomRevDotted,
}

impl SeqArrow {
    /// `true` for a reverse directional arrow (head at the source end).
    pub fn is_reverse(self) -> bool {
        matches!(
            self,
            SeqArrow::SolidTopRev
                | SeqArrow::SolidBottomRev
                | SeqArrow::SolidTopRevDotted
                | SeqArrow::SolidBottomRevDotted
                | SeqArrow::StickTopRev
                | SeqArrow::StickBottomRev
                | SeqArrow::StickTopRevDotted
                | SeqArrow::StickBottomRevDotted
        )
    }
    /// `true` if the line is dotted (rendered with a dash pattern).
    pub fn is_dotted(self) -> bool {
        matches!(
            self,
            SeqArrow::DottedArrow
                | SeqArrow::DottedOpen
                | SeqArrow::DottedCross
                | SeqArrow::DottedPoint
                | SeqArrow::BiDotted
                | SeqArrow::SolidTopDotted
                | SeqArrow::SolidBottomDotted
                | SeqArrow::SolidTopRevDotted
                | SeqArrow::SolidBottomRevDotted
                | SeqArrow::StickTopDotted
                | SeqArrow::StickBottomDotted
                | SeqArrow::StickTopRevDotted
                | SeqArrow::StickBottomRevDotted
        )
    }
}
