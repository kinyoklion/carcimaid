use dagre::graph::{Graph, GraphOptions};
use dagre::layout::types::{EdgeLabel, LayoutOptions, NodeLabel, RankDir};
use dagre::layout::layout as dagre_layout;

fn main() {
    // Compound graph: cluster "c" containing a1 -> a2, laid out LR.
    let mut g: Graph<NodeLabel, EdgeLabel> =
        Graph::with_options(GraphOptions { directed: true, multigraph: true, compound: true });
    for id in ["a1", "a2"] {
        g.set_node(id.to_string(), Some(NodeLabel { width: 79.984375, height: 49.0, ..Default::default() }));
    }
    g.set_node("c".to_string(), Some(NodeLabel::default()));
    g.set_parent("a1", Some("c"));
    g.set_parent("a2", Some("c"));
    g.set_edge("a1".to_string(), "a2".to_string(), Some(EdgeLabel::default()), Some("e0"));
    dagre_layout(&mut g, Some(LayoutOptions {
        rankdir: RankDir::LR, nodesep: 50.0, ranksep: 50.0, edgesep: 20.0,
        marginx: 8.0, marginy: 8.0, tie_keep_first: true, ..Default::default()
    }));
    let a1 = g.node("a1").unwrap();
    let a2 = g.node("a2").unwrap();
    let c = g.node("c").unwrap();
    println!("a1 = ({:.2}, {:.2})", a1.x.unwrap(), a1.y.unwrap());
    println!("a2 = ({:.2}, {:.2})", a2.x.unwrap(), a2.y.unwrap());
    println!("gap = {:.2}  (mermaid: 154.99)", a2.x.unwrap() - a1.x.unwrap());
    println!("cluster c = pos({:.2},{:.2}) size {:.2} x {:.2}  (mermaid box: 309.97 x 119)",
        c.x.unwrap(), c.y.unwrap(), c.width, c.height);
}
