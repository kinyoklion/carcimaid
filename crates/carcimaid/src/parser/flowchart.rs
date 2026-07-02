//! A pragmatic parser for mermaid `flowchart` / `graph` source.
//!
//! This is deliberately a hand-written subset that grows as the compliance
//! corpus exercises more syntax. It currently handles: the direction header,
//! node definitions with the common shape brackets, edge chains
//! (`A --> B --> C`), the main edge operators, and `-->|label|` edge labels.
//!
//! Not yet handled (tracked for future work): subgraphs, `classDef`/`class`/
//! `style`, `click`, `&` multi-node statements, and the `A -- text --> B`
//! inline-label form. These are recognised loosely and skipped rather than
//! erroring, so partially-supported diagrams still produce output.

use crate::ir::{Direction, Edge, EdgeStyle, Flowchart, Node, NodeShape, Subgraph};
use crate::Result;

/// Parse a flowchart from full source (including its header line).
pub fn parse(source: &str) -> Result<Flowchart> {
    let mut chart = Flowchart::default();
    let mut lines = source.lines().map(str::trim).filter(|l| !l.is_empty());

    // Header: `flowchart TD` / `graph LR`. The keyword is already validated by
    // the dispatcher; here we only extract the optional direction token.
    if let Some(header) = lines.next() {
        if let Some(dir) = header.split_whitespace().nth(1) {
            chart.direction = parse_direction(dir).unwrap_or_default();
        }
    }

    // Stack of enclosing subgraph indices (for nesting). The top is the current
    // subgraph that newly-defined nodes are assigned to.
    let mut stack: Vec<usize> = Vec::new();

    for line in lines {
        if line.starts_with("%%") {
            continue;
        }
        for stmt in line.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            if let Some(rest) = subgraph_header(stmt) {
                let parent = stack.last().copied();
                let (id, title) = parse_subgraph_header(rest, chart.subgraphs.len());
                chart.subgraphs.push(Subgraph { id, title, parent });
                stack.push(chart.subgraphs.len() - 1);
            } else if stmt == "end" {
                stack.pop();
            } else if is_directive_stmt(stmt) {
                // Styling/interaction directives (`classDef`, `class`, `style`,
                // `linkStyle`, `click`) and `direction` are not nodes/edges;
                // skip them so they don't become phantom nodes. Their visual
                // effect (classes/styles) is out of scope for structural diffing.
            } else {
                parse_statement(stmt, &mut chart, stack.last().copied());
            }
        }
    }

    Ok(chart)
}

/// If `stmt` opens a subgraph, return the text after the `subgraph` keyword.
fn subgraph_header(stmt: &str) -> Option<&str> {
    let rest = stmt.strip_prefix("subgraph")?;
    // Must be followed by whitespace or end (avoid matching an id like `subgraphX`).
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        Some(rest.trim())
    } else {
        None
    }
}

/// A statement that is a styling/interaction directive rather than a node or
/// edge (matched on its first whitespace-delimited keyword).
fn is_directive_stmt(stmt: &str) -> bool {
    matches!(
        stmt.split_whitespace().next(),
        Some("direction" | "classDef" | "class" | "style" | "linkStyle" | "click")
    )
}

/// Parse a subgraph header body into (id, title). Forms: `` (anonymous),
/// `Title`, `id[Title]`, `"Title"`. Anonymous subgraphs get a synthetic id.
fn parse_subgraph_header(body: &str, index: usize) -> (String, String) {
    let body = body.trim();
    if body.is_empty() {
        return (format!("subGraph{index}"), String::new());
    }
    if let Some(open) = body.find('[') {
        if let Some(inner) = body[open..].strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            let id = body[..open].trim().to_string();
            return (id, unquote(inner).to_string());
        }
    }
    let t = unquote(body).to_string();
    (t.clone(), t)
}

fn parse_direction(token: &str) -> Option<Direction> {
    match token {
        "TD" | "TB" => Some(Direction::TopBottom),
        "BT" => Some(Direction::BottomTop),
        "LR" => Some(Direction::LeftRight),
        "RL" => Some(Direction::RightLeft),
        _ => None,
    }
}

/// Parse a single statement: either a bare node definition or an edge chain.
///
/// Each endpoint may be an `&`-separated group (`A & B --> C & D`), which
/// expands to the cross-product of edges between adjacent groups.
fn parse_statement(stmt: &str, chart: &mut Flowchart, current: Option<usize>) {
    let (endpoints, ops) = split_chain(stmt);
    let groups: Vec<Vec<usize>> = endpoints
        .iter()
        .map(|ep| split_group(ep).iter().map(|n| ensure_node(chart, n, current)).collect())
        .collect();

    if ops.is_empty() {
        // Bare node definition(s), e.g. `A[Start]` or `A & B`.
        return;
    }

    for (i, op) in ops.iter().enumerate() {
        for &from in &groups[i] {
            for &to in &groups[i + 1] {
                chart.edges.push(Edge {
                    from,
                    to,
                    label: op.label.clone(),
                    style: op.style,
                    arrow: op.arrow,
                });
            }
        }
    }
}

/// Split an endpoint into `&`-separated node tokens, respecting bracket/quote
/// nesting so an `&` inside a label is not treated as a separator.
fn split_group(endpoint: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    for c in endpoint.chars() {
        match c {
            '"' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            '[' | '(' | '{' if !in_quote => {
                depth += 1;
                cur.push(c);
            }
            ']' | ')' | '}' if !in_quote => {
                depth -= 1;
                cur.push(c);
            }
            '&' if depth == 0 && !in_quote => {
                if !cur.trim().is_empty() {
                    parts.push(cur.trim().to_string());
                }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur.trim().to_string());
    }
    parts
}

/// A parsed edge operator with its optional `|label|`.
struct EdgeOp {
    style: EdgeStyle,
    arrow: bool,
    label: Option<String>,
}

/// Split a statement into endpoint strings and the operators between them,
/// respecting bracket/quote nesting so operator glyphs inside labels are
/// ignored. Returns (endpoint_strings, operators) where
/// `endpoint_strings.len() == operators.len() + 1` for a well-formed chain.
fn split_chain(stmt: &str) -> (Vec<String>, Vec<EdgeOp>) {
    let mut endpoints = Vec::new();
    let mut ops = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_quote = false;

    // `i` is always a byte offset on a char boundary: we only ever advance by
    // a whole char's UTF-8 length or by an (ASCII) operator's byte length.
    let mut i = 0;
    while i < stmt.len() {
        let c = stmt[i..].chars().next().unwrap();
        let clen = c.len_utf8();
        if in_quote {
            cur.push(c);
            if c == '"' {
                in_quote = false;
            }
            i += clen;
            continue;
        }
        match c {
            '"' => {
                in_quote = true;
                cur.push(c);
                i += clen;
            }
            '[' | '(' | '{' => {
                depth += 1;
                cur.push(c);
                i += clen;
            }
            ']' | ')' | '}' => {
                depth -= 1;
                cur.push(c);
                i += clen;
            }
            _ if depth == 0 => {
                if let Some((len, style, arrow)) = detect_op(&stmt[i..]) {
                    endpoints.push(cur.trim().to_string());
                    cur = String::new();
                    i += len;
                    // Optional `|label|` immediately after the operator.
                    let rest = stmt[i..].trim_start();
                    let label = if rest.starts_with('|') {
                        let consumed = stmt.len() - rest.len();
                        let after_bar = &rest[1..];
                        if let Some(end) = after_bar.find('|') {
                            i = consumed + 1 + end + 1;
                            Some(after_bar[..end].trim().to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    ops.push(EdgeOp { style, arrow, label });
                } else {
                    cur.push(c);
                    i += clen;
                }
            }
            _ => {
                cur.push(c);
                i += clen;
            }
        }
    }
    endpoints.push(cur.trim().to_string());
    (endpoints, ops)
}

/// Detect an edge operator at the start of `s`, returning its byte length,
/// style, and whether it has an arrowhead. Longer operators are matched first.
fn detect_op(s: &str) -> Option<(usize, EdgeStyle, bool)> {
    const OPS: &[(&str, EdgeStyle, bool)] = &[
        ("-.->", EdgeStyle::Dotted, true),
        ("-.-", EdgeStyle::Dotted, false),
        ("==>", EdgeStyle::Thick, true),
        ("===", EdgeStyle::Thick, false),
        ("-->", EdgeStyle::Solid, true),
        ("---", EdgeStyle::Solid, false),
    ];
    OPS.iter()
        .find(|(pat, ..)| s.starts_with(pat))
        .map(|&(pat, style, arrow)| (pat.len(), style, arrow))
}

/// Ensure a node parsed from `endpoint` exists in the chart, returning its
/// index. If the endpoint carries a shape/label, it updates the existing node.
/// A newly-created node is assigned to `current` (the enclosing subgraph).
fn ensure_node(chart: &mut Flowchart, endpoint: &str, current: Option<usize>) -> usize {
    let (id, shape, label) = parse_endpoint(endpoint);
    if let Some(idx) = chart.node_index(&id) {
        if let Some(label) = label {
            chart.nodes[idx].label = label;
            chart.nodes[idx].shape = shape;
        }
        // A node belongs to the first subgraph whose block references it, even
        // if it was defined earlier (e.g. by an edge before the block).
        if current.is_some() && chart.nodes[idx].subgraph.is_none() {
            chart.nodes[idx].subgraph = current;
        }
        return idx;
    }
    let label = label.unwrap_or_else(|| id.clone());
    chart.nodes.push(Node { id, label, shape, subgraph: current });
    chart.nodes.len() - 1
}

/// Strip a trailing `:::className` (inline class assignment) that appears at
/// bracket depth 0, leaving the node id/shape spec.
fn strip_class(endpoint: &str) -> &str {
    let mut depth = 0i32;
    let mut i = 0;
    while i < endpoint.len() {
        let c = endpoint[i..].chars().next().unwrap();
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ':' if depth == 0 && endpoint[i..].starts_with(":::") => {
                return endpoint[..i].trim();
            }
            _ => {}
        }
        i += c.len_utf8();
    }
    endpoint
}

/// Parse an endpoint token into (id, shape, optional label).
fn parse_endpoint(endpoint: &str) -> (String, NodeShape, Option<String>) {
    let endpoint = strip_class(endpoint.trim());
    // Find where the shape bracket (if any) begins.
    let open = endpoint.find(['[', '(', '{']);
    let Some(open) = open else {
        return (endpoint.to_string(), NodeShape::Rectangle, None);
    };

    let id = endpoint[..open].trim().to_string();
    let rest = &endpoint[open..];
    let (shape, inner) = if let Some(inner) = rest.strip_prefix("((").and_then(|r| r.strip_suffix("))")) {
        (NodeShape::Circle, inner)
    } else if let Some(inner) = rest.strip_prefix("([").and_then(|r| r.strip_suffix("])")) {
        (NodeShape::Stadium, inner)
    } else if let Some(inner) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        (NodeShape::Rectangle, inner)
    } else if let Some(inner) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        (NodeShape::RoundedRectangle, inner)
    } else if let Some(inner) = rest.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        (NodeShape::Rhombus, inner)
    } else {
        // Unbalanced/unknown bracketing — treat the whole token as an id.
        return (endpoint.to_string(), NodeShape::Rectangle, None);
    };

    (id, shape, Some(unquote(inner).to_string()))
}

/// Strip a single pair of surrounding double quotes from a label, if present.
fn unquote(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_direction_and_simple_edge() {
        let chart = parse("flowchart LR\n  A[Start] --> B{Choice}").unwrap();
        assert_eq!(chart.direction, Direction::LeftRight);
        assert_eq!(chart.nodes.len(), 2);
        assert_eq!(chart.nodes[0].id, "A");
        assert_eq!(chart.nodes[0].label, "Start");
        assert_eq!(chart.nodes[0].shape, NodeShape::Rectangle);
        assert_eq!(chart.nodes[1].shape, NodeShape::Rhombus);
        assert_eq!(chart.edges.len(), 1);
        assert!(chart.edges[0].arrow);
    }

    #[test]
    fn parses_edge_label_and_chain() {
        let chart = parse("flowchart TD\n A --> B -->|next| C").unwrap();
        assert_eq!(chart.nodes.len(), 3);
        assert_eq!(chart.edges.len(), 2);
        assert_eq!(chart.edges[1].label.as_deref(), Some("next"));
    }

    #[test]
    fn dotted_and_thick_styles() {
        let chart = parse("graph TD\n A -.-> B\n B ==> C").unwrap();
        assert_eq!(chart.edges[0].style, EdgeStyle::Dotted);
        assert_eq!(chart.edges[1].style, EdgeStyle::Thick);
    }

    #[test]
    fn skips_directives_and_inline_class() {
        let chart = parse(
            "graph TD\n classDef default fill:#a34,stroke:#000\n hello --> default\n style hello fill:#f00\n click hello \"http://x\"",
        )
        .unwrap();
        // Only the two real nodes exist; no phantom `classDef`/`style`/`click`.
        let ids: Vec<_> = chart.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["hello", "default"]);
    }

    #[test]
    fn strips_inline_class_suffix() {
        let chart = parse("flowchart TD\n A:::foo --> B[label]:::bar").unwrap();
        assert_eq!(chart.node_index("A"), Some(0));
        assert_eq!(chart.nodes[1].id, "B");
        assert_eq!(chart.nodes[1].label, "label");
    }

    #[test]
    fn parses_subgraph_membership() {
        let chart = parse("flowchart TB\n subgraph One\n a1 --> a2\n end\n a2 --> b1").unwrap();
        assert_eq!(chart.subgraphs.len(), 1);
        assert_eq!(chart.subgraphs[0].id, "One");
        let sg = chart.node_index("a1").map(|i| chart.nodes[i].subgraph).unwrap();
        assert_eq!(sg, Some(0));
        // b1 is defined outside the subgraph.
        let b1 = chart.node_index("b1").map(|i| chart.nodes[i].subgraph).unwrap();
        assert_eq!(b1, None);
    }

    #[test]
    fn subgraph_with_bracket_title() {
        let chart = parse("flowchart TB\n subgraph s1[My Title]\n a\n end").unwrap();
        assert_eq!(chart.subgraphs[0].id, "s1");
        assert_eq!(chart.subgraphs[0].title, "My Title");
    }

    #[test]
    fn expands_ampersand_node_groups() {
        // `A & B --> C & D` is the cross-product of edges.
        let chart = parse("flowchart TD\n A & B --> C & D").unwrap();
        assert_eq!(chart.nodes.len(), 4);
        assert_eq!(chart.edges.len(), 4); // A-C, A-D, B-C, B-D
        let ids: Vec<_> = chart.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["A", "B", "C", "D"]);
    }

    #[test]
    fn handles_multibyte_utf8_labels() {
        // Regression: byte-indexed scanning used to panic on non-ASCII labels.
        let chart = parse("flowchart TD\n a[\"提交\"] --> b[\"完成\"]").unwrap();
        assert_eq!(chart.nodes.len(), 2);
        assert_eq!(chart.nodes[0].label, "提交");
        assert_eq!(chart.nodes[1].label, "完成");
        assert_eq!(chart.edges.len(), 1);
    }

    #[test]
    fn ignores_operator_glyphs_inside_labels() {
        let chart = parse("flowchart TD\n A[a --> b] --> B").unwrap();
        assert_eq!(chart.nodes.len(), 2);
        assert_eq!(chart.nodes[0].label, "a --> b");
        assert_eq!(chart.edges.len(), 1);
    }
}
