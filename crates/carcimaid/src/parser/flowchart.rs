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

use crate::ir::{Direction, Edge, EdgeStyle, Flowchart, Node, NodeShape};
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

    for line in lines {
        if line.starts_with("%%") {
            continue;
        }
        for stmt in line.split(';') {
            let stmt = stmt.trim();
            if !stmt.is_empty() {
                parse_statement(stmt, &mut chart);
            }
        }
    }

    Ok(chart)
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
fn parse_statement(stmt: &str, chart: &mut Flowchart) {
    let (endpoints, ops) = split_chain(stmt);

    if ops.is_empty() {
        // Bare node definition, e.g. `A[Start]`.
        if let Some(ep) = endpoints.first() {
            ensure_node(chart, ep);
        }
        return;
    }

    for (i, op) in ops.iter().enumerate() {
        let from = ensure_node(chart, &endpoints[i]);
        let to = ensure_node(chart, &endpoints[i + 1]);
        chart.edges.push(Edge {
            from,
            to,
            label: op.label.clone(),
            style: op.style,
            arrow: op.arrow,
        });
    }
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
    let bytes = stmt.as_bytes();
    let mut endpoints = Vec::new();
    let mut ops = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_quote = false;

    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_quote {
            cur.push(c);
            if c == '"' {
                in_quote = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_quote = true;
                cur.push(c);
                i += 1;
            }
            '[' | '(' | '{' => {
                depth += 1;
                cur.push(c);
                i += 1;
            }
            ']' | ')' | '}' => {
                depth -= 1;
                cur.push(c);
                i += 1;
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
                    i += 1;
                }
            }
            _ => {
                cur.push(c);
                i += 1;
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
fn ensure_node(chart: &mut Flowchart, endpoint: &str) -> usize {
    let (id, shape, label) = parse_endpoint(endpoint);
    if let Some(idx) = chart.node_index(&id) {
        if let Some(label) = label {
            chart.nodes[idx].label = label;
            chart.nodes[idx].shape = shape;
        }
        return idx;
    }
    let label = label.unwrap_or_else(|| id.clone());
    chart.nodes.push(Node { id, label, shape });
    chart.nodes.len() - 1
}

/// Parse an endpoint token into (id, shape, optional label).
fn parse_endpoint(endpoint: &str) -> (String, NodeShape, Option<String>) {
    let endpoint = endpoint.trim();
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
    fn ignores_operator_glyphs_inside_labels() {
        let chart = parse("flowchart TD\n A[a --> b] --> B").unwrap();
        assert_eq!(chart.nodes.len(), 2);
        assert_eq!(chart.nodes[0].label, "a --> b");
        assert_eq!(chart.edges.len(), 1);
    }
}
