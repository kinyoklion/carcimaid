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

use crate::ir::{ArrowType, Direction, Edge, EdgeStyle, Flowchart, Node, NodeShape, Subgraph};
use crate::Result;

/// Parse a flowchart from full source (including its header line).
pub fn parse(source: &str) -> Result<Flowchart> {
    let mut chart = Flowchart::default();

    // Stack of enclosing subgraph indices (for nesting). The top is the current
    // subgraph that newly-defined nodes are assigned to.
    let mut stack: Vec<usize> = Vec::new();
    let mut header_seen = false;

    for stmt in split_statements(source) {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        if !header_seen {
            // Header: `flowchart TD` / `graph LR` — take only the direction.
            header_seen = true;
            if let Some(dir) = stmt.split_whitespace().nth(1) {
                chart.direction = parse_direction(dir).unwrap_or_default();
            }
            continue;
        }
        // Accessibility metadata renders as <title>/<desc>, not as a node.
        if let Some(rest) = stmt.strip_prefix("accTitle") {
            chart.acc_title = Some(rest.trim().trim_start_matches(':').trim().to_string());
            continue;
        }
        if let Some(rest) = stmt.strip_prefix("accDescr") {
            let rest = rest.trim();
            let text = if let Some(inner) = rest.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
                inner.trim() // multi-line `accDescr { … }`
            } else {
                rest.trim_start_matches(':').trim()
            };
            chart.acc_descr = Some(text.to_string());
            continue;
        }
        if let Some(rest) = subgraph_header(stmt) {
            let parent = stack.last().copied();
            let (id, title) = parse_subgraph_header(rest, chart.subgraphs.len());
            chart.subgraphs.push(Subgraph {
                id,
                title,
                parent,
                direction: None,
                classes: Vec::new(),
                styles: Vec::new(),
            });
            stack.push(chart.subgraphs.len() - 1);
        } else if stmt == "end" {
            stack.pop();
        } else if let Some(dir) = stmt.strip_prefix("direction").filter(|r| r.starts_with(char::is_whitespace)) {
            // `direction XX` inside a subgraph sets that subgraph's direction. A
            // top-level `direction` does NOT override the header (mermaid keeps
            // the header direction for the root), so it is ignored here.
            if let (Some(&top), Some(d)) = (stack.last(), parse_direction(dir.trim())) {
                chart.subgraphs[top].direction = Some(d);
            }
        } else if let Some(rest) = strip_kw(stmt, "classDef") {
            parse_class_def(rest, &mut chart);
        } else if let Some(rest) = strip_kw(stmt, "class") {
            parse_class_apply(rest, &mut chart);
        } else if let Some(rest) = strip_kw(stmt, "style") {
            parse_style(rest, &mut chart);
        } else if let Some(rest) = strip_kw(stmt, "linkStyle") {
            parse_link_style(rest, &mut chart);
        } else if is_directive_stmt(stmt) {
            // Styling/interaction directives (`classDef`, `class`, `style`,
            // `linkStyle`, `click`) and `direction` are not nodes/edges; skip
            // them so they don't become phantom nodes.
        } else {
            parse_statement(stmt, &mut chart, stack.last().copied());
        }
    }

    resolve_subgraph_refs(&mut chart);
    Ok(chart)
}

/// Mark any node whose id matches a subgraph id as a subgraph reference. This
/// happens when a subgraph name is used as an edge endpoint (`X --> Y`): the
/// edge is parsed before the `subgraph X`/`subgraph Y` blocks, so `ensure_node`
/// creates a phantom node for it. mermaid renders no node in that case — the
/// edge attaches to the cluster — so we flag the node and let layout/render
/// treat the endpoint as the subgraph. (A subgraph id shadows a node id.)
fn resolve_subgraph_refs(chart: &mut Flowchart) {
    use std::collections::HashMap;
    let sg_by_id: HashMap<String, usize> = chart
        .subgraphs
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();
    for node in &mut chart.nodes {
        if let Some(&s) = sg_by_id.get(&node.id) {
            node.subgraph_ref = Some(s);
        }
    }
}

/// Split source into statements. A statement ends at `;` or a newline, but only
/// at bracket depth 0 and outside quotes — so multi-line quoted labels (and
/// `accDescr { … }` blocks) stay intact. `%%` starts a line comment.
fn split_statements(source: &str) -> Vec<String> {
    let mut stmts = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quote {
            cur.push(c);
            if c == '"' {
                in_quote = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_quote = true;
                cur.push(c);
            }
            '%' if depth == 0 && chars.peek() == Some(&'%') => {
                // Comment to end of line.
                while chars.peek().is_some_and(|&n| n != '\n') {
                    chars.next();
                }
            }
            '[' | '(' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ']' | ')' | '}' => {
                depth -= 1;
                cur.push(c);
            }
            ';' | '\n' if depth <= 0 => stmts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    stmts.push(cur);
    stmts
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

/// If `stmt` begins with keyword `kw` followed by whitespace, return the rest.
fn strip_kw<'a>(stmt: &'a str, kw: &str) -> Option<&'a str> {
    stmt.strip_prefix(kw).filter(|r| r.starts_with(char::is_whitespace)).map(str::trim)
}

/// Split a comma-separated CSS declaration list into trimmed `k:v` strings,
/// dropping empties and stray `!important` markers (added back at render time).
fn split_css(props: &str) -> Vec<String> {
    props
        .split(',')
        .map(|p| p.replace("!important", "").trim().trim_end_matches(';').trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// `classDef <names> <props>` — one or more comma-separated class names sharing
/// a CSS declaration list.
fn parse_class_def(rest: &str, chart: &mut Flowchart) {
    let Some((names, props)) = rest.split_once(char::is_whitespace) else {
        return;
    };
    let decls = split_css(props);
    for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        chart.class_defs.entry(name.to_string()).or_default().extend(decls.iter().cloned());
    }
}

/// `class <ids> <className>` — apply a class to one or more nodes/subgraphs.
fn parse_class_apply(rest: &str, chart: &mut Flowchart) {
    let Some((ids, name)) = rest.rsplit_once(char::is_whitespace) else {
        return;
    };
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    for id in ids.split(',').map(str::trim).filter(|i| !i.is_empty()) {
        add_class(chart, id, name);
    }
}

/// `style <id> <props>` — direct inline styles on a node or subgraph.
fn parse_style(rest: &str, chart: &mut Flowchart) {
    let Some((id, props)) = rest.split_once(char::is_whitespace) else {
        return;
    };
    let decls = split_css(props);
    if let Some(idx) = chart.node_index(id.trim()) {
        chart.nodes[idx].styles.extend(decls);
    } else if let Some(sg) = chart.subgraphs.iter_mut().find(|s| s.id == id.trim()) {
        sg.styles.extend(decls);
    }
}

/// `linkStyle <default|indices> <props>` — style specific edges (by 0-based
/// index) or all edges (`default`). `interpolate ...` clauses are ignored.
fn parse_link_style(rest: &str, chart: &mut Flowchart) {
    let Some((spec, props)) = rest.split_once(char::is_whitespace) else {
        return;
    };
    // Drop a trailing `interpolate <fn>` clause if present before the props.
    let props = props.strip_prefix("interpolate").map_or(props, |r| {
        r.trim_start().split_once(char::is_whitespace).map_or("", |(_, p)| p)
    });
    let decls = split_css(props);
    if decls.is_empty() {
        return;
    }
    if spec.trim() == "default" {
        // Applies to every edge; kept separate so a per-index linkStyle appends
        // to it rather than replacing it (mermaid concatenates default + index).
        chart.link_style_default = decls;
    } else {
        for idx in spec.split(',').filter_map(|i| i.trim().parse::<usize>().ok()) {
            if let Some(e) = chart.edges.get_mut(idx) {
                e.link_style = decls.clone();
            }
        }
    }
}

/// Add a class name to a node (or subgraph) by id.
fn add_class(chart: &mut Flowchart, id: &str, name: &str) {
    if let Some(idx) = chart.node_index(id) {
        if !chart.nodes[idx].classes.iter().any(|c| c == name) {
            chart.nodes[idx].classes.push(name.to_string());
        }
    } else if let Some(sg) = chart.subgraphs.iter_mut().find(|s| s.id == id) {
        if !sg.classes.iter().any(|c| c == name) {
            sg.classes.push(name.to_string());
        }
    }
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
                    arrow_start: op.start,
                    arrow_end: op.end,
                    link_style: Vec::new(),
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

/// A parsed edge operator with its optional label.
struct EdgeOp {
    style: EdgeStyle,
    start: ArrowType,
    end: ArrowType,
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
                // `o`/`x` start arrows are only valid at a token boundary, else
                // they'd match the trailing letter of an id like `foo-->` / `box-->`.
                let boundary = cur.chars().last().is_none_or(char::is_whitespace);
                if let Some((len, style, start, end, mid)) = detect_link(&stmt[i..], boundary) {
                    endpoints.push(cur.trim().to_string());
                    cur = String::new();
                    i += len;
                    // Label: the `-- text -->` middle text, else a `|label|`
                    // immediately after the operator.
                    let label = if mid.is_some() {
                        mid
                    } else {
                        let rest = stmt[i..].trim_start();
                        if rest.starts_with('|') {
                            let consumed = stmt.len() - rest.len();
                            let after_bar = &rest[1..];
                            if let Some(bar) = after_bar.find('|') {
                                i = consumed + 1 + bar + 1;
                                Some(after_bar[..bar].trim().to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    ops.push(EdgeOp { style, start, end, label });
                } else {
                    // The odd/flag shape `id>text]` opens with `>` and closes
                    // with `]`; count it as a bracket so the trailing `]` keeps
                    // `depth` balanced and doesn't hide a following link operator.
                    if c == '>' {
                        depth += 1;
                    }
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

/// Detect an edge link at the start of `s`, returning its byte length, style,
/// start/end arrow types, and any `-- text --` middle label. Handles solid
/// (`--`), thick (`==`), and dotted (`-.-`) links, optional start arrows
/// (`<`/`x`/`o`), optional end arrows (`>`/`x`/`o`), and inline middle text.
fn detect_link(s: &str, boundary: bool) -> Option<(usize, EdgeStyle, ArrowType, ArrowType, Option<String>)> {
    let b = s.as_bytes();
    let mut i = 0;

    // Optional start arrow. `x`/`o` are only arrows at a token boundary (else
    // they'd match the last letter of an id like `foo-->`); `<` always is.
    let start = match b.first().copied() {
        Some(b'<') => {
            i = 1;
            ArrowType::Point
        }
        Some(b'x') if boundary && matches!(b.get(1), Some(b'-') | Some(b'=')) => {
            i = 1;
            ArrowType::Cross
        }
        Some(b'o') if boundary && matches!(b.get(1), Some(b'-') | Some(b'=')) => {
            i = 1;
            ArrowType::Circle
        }
        _ => ArrowType::None,
    };
    let end_arrow = |p: usize| -> Option<(ArrowType, usize)> {
        match b.get(p).copied() {
            Some(b'>') => Some((ArrowType::Point, p + 1)),
            Some(b'x') => Some((ArrowType::Cross, p + 1)),
            Some(b'o') => Some((ArrowType::Circle, p + 1)),
            _ => None,
        }
    };

    // Dotted (`-.-`, `-.->`, `-. text .->`).
    if b.get(i) == Some(&b'-') && b.get(i + 1) == Some(&b'.') {
        i += 2; // opening "-."
        if b.get(i) == Some(&b'-') {
            i += 1; // closing dash
            let (end, j) = end_arrow(i).unwrap_or((ArrowType::None, i));
            return Some((j, EdgeStyle::Dotted, start, end, None));
        }
        // Middle text delimited by the closing ".-".
        if let Some(rel) = s[i..].find(".-") {
            let text = s[i..i + rel].trim().to_string();
            let after = i + rel + 2;
            let (end, j) = end_arrow(after).unwrap_or((ArrowType::None, after));
            return Some((j, EdgeStyle::Dotted, start, end, (!text.is_empty()).then_some(text)));
        }
        return None;
    }

    // Solid (`-`) or thick (`=`).
    let line = match b.get(i).copied() {
        Some(b'=') => b'=',
        Some(b'-') => b'-',
        _ => return None,
    };
    let style = if line == b'=' { EdgeStyle::Thick } else { EdgeStyle::Solid };
    let open_start = i;
    while b.get(i) == Some(&line) {
        i += 1;
    }
    if i - open_start < 2 && start == ArrowType::None {
        return None; // a lone `-`/`=` is not a link
    }
    // Immediate end arrow: `-->`, `==>`, `--x`, `<-->`, …
    if let Some((end, j)) = end_arrow(i) {
        return Some((j, style, start, end, None));
    }
    // Middle text: text up to the next line run, then that run + optional arrow.
    let close = if line == b'=' { "==" } else { "--" };
    if let Some(rel) = s[i..].find(close) {
        let text = s[i..i + rel].trim().to_string();
        if !text.is_empty() {
            let mut j = i + rel;
            while b.get(j) == Some(&line) {
                j += 1;
            }
            let (end, j2) = end_arrow(j).unwrap_or((ArrowType::None, j));
            return Some((j2, style, start, end, Some(text)));
        }
    }
    // Bare open link (`--`, `---`, `==`, `===`).
    Some((i, style, start, ArrowType::None, None))
}

/// Ensure a node parsed from `endpoint` exists in the chart, returning its
/// index. If the endpoint carries a shape/label, it updates the existing node.
/// A newly-created node is assigned to `current` (the enclosing subgraph).
fn ensure_node(chart: &mut Flowchart, endpoint: &str, current: Option<usize>) -> usize {
    let (id, shape, label, class) = parse_endpoint(endpoint);
    let idx = if let Some(idx) = chart.node_index(&id) {
        if let Some(label) = label {
            chart.nodes[idx].label = label;
            chart.nodes[idx].shape = shape;
        }
        // A node belongs to the first subgraph whose block references it, even
        // if it was defined earlier (e.g. by an edge before the block).
        if current.is_some() && chart.nodes[idx].subgraph.is_none() {
            chart.nodes[idx].subgraph = current;
        }
        idx
    } else {
        let label = label.unwrap_or_else(|| id.clone());
        chart.nodes.push(Node { id, label, shape, subgraph: current, classes: Vec::new(), styles: Vec::new(), subgraph_ref: None });
        chart.nodes.len() - 1
    };
    // Inline `id:::className` class assignment.
    if let Some(class) = class {
        if !chart.nodes[idx].classes.contains(&class) {
            chart.nodes[idx].classes.push(class);
        }
    }
    idx
}

/// Split a trailing `:::className` (inline class assignment) at bracket depth 0,
/// returning the node id/shape spec and the class name (if any).
fn split_class(endpoint: &str) -> (&str, Option<String>) {
    let mut depth = 0i32;
    let mut i = 0;
    while i < endpoint.len() {
        let c = endpoint[i..].chars().next().unwrap();
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ':' if depth == 0 && endpoint[i..].starts_with(":::") => {
                let class = endpoint[i + 3..].trim();
                return (endpoint[..i].trim(), (!class.is_empty()).then(|| class.to_string()));
            }
            _ => {}
        }
        i += c.len_utf8();
    }
    (endpoint, None)
}

/// Map a mermaid v11 shape name/alias to a [`NodeShape`]. Unknown shapes fall
/// back to a rectangle. (Shapes we don't model exactly reuse the closest one.)
fn map_shape(name: &str) -> NodeShape {
    match name {
        "circle" | "circ" => NodeShape::Circle,
        "rounded" | "event" => NodeShape::RoundedRectangle,
        "stadium" | "pill" | "terminal" => NodeShape::Stadium,
        "diam" | "diamond" | "decision" | "question" => NodeShape::Rhombus,
        "hex" | "hexagon" | "prepare" => NodeShape::Hexagon,
        "subproc" | "subprocess" | "subroutine" | "framed-rectangle" | "fr-rect" => {
            NodeShape::Subroutine
        }
        "cyl" | "cylinder" | "database" | "db" | "disk" => NodeShape::Cylinder,
        // The data-store symbol renders as an open-ended (dashed-side) rect, not
        // a 3D cylinder — mermaid emits `<rect stroke-dasharray="w h">`.
        "datastore" | "das" => NodeShape::DataStore,
        "sm-circ" | "small-circle" => NodeShape::SmallCircle,
        "dbl-circ" | "double-circle" => NodeShape::DoubleCircle,
        "div-rect" | "div-proc" | "divided-rectangle" | "divided-process" => NodeShape::DividedRect,
        "lin-rect" | "lined-rectangle" | "lined-process" | "lin-proc" | "shaded-process" => {
            NodeShape::LinedProcess
        }
        "win-pane" | "window-pane" | "internal-storage" => NodeShape::WindowPane,
        "st-rect" | "procs" | "processes" | "stacked-rectangle" => NodeShape::StackedRect,
        "odd" => NodeShape::Odd,
        "lean-r" | "lean-right" | "in-out" | "lin-r" => NodeShape::Parallelogram,
        "lean-l" | "lean-left" | "out-in" | "lin-l" => NodeShape::LeanLeft,
        "trap-b" | "trapezoid" | "trapezoid-bottom" | "manual" => NodeShape::Trapezoid,
        "trap-t" | "inv-trapezoid" | "trapezoid-top" | "priority" => NodeShape::InvTrapezoid,
        _ => NodeShape::Rectangle, // rect/rectangle/box/proc/process/… and unknowns
    }
}

/// Parse the body of a v11 `@{ … }` node-metadata block, e.g.
/// `shape: datastore, label: "Datastore"`, into (shape, optional label).
fn parse_at_metadata(inner: &str) -> (NodeShape, Option<String>) {
    let mut shape = NodeShape::Rectangle;
    let mut label = None;
    let mut cur = String::new();
    let mut in_quote = false;
    let mut parts = Vec::new();
    for c in inner.chars() {
        match c {
            '"' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            ',' if !in_quote => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    for part in parts {
        if let Some((k, v)) = part.split_once(':') {
            let v = unquote(v.trim());
            match k.trim() {
                "shape" => shape = map_shape(v),
                "label" | "title" => label = Some(v.to_string()),
                _ => {}
            }
        }
    }
    (shape, label)
}

/// Parse an endpoint token into (id, shape, optional label).
fn parse_endpoint(endpoint: &str) -> (String, NodeShape, Option<String>, Option<String>) {
    let (endpoint, class) = split_class(endpoint.trim());
    // mermaid v11 `id@{ shape: …, label: "…" }` node-metadata syntax.
    if let Some(at) = endpoint.find("@{") {
        let id = endpoint[..at].trim().to_string();
        let inner = endpoint[at + 2..].trim().strip_suffix('}').unwrap_or(&endpoint[at + 2..]);
        let (shape, label) = parse_at_metadata(inner);
        return (id, shape, label, class);
    }
    // Odd/flag shape `id>text]`: a `>` opener (before any normal bracket) with a
    // `]` closer. mermaid renders it as a notched-left path; we approximate it.
    if let Some(gt) = endpoint.find('>') {
        if endpoint.ends_with(']') && !endpoint[..gt].contains(['[', '(', '{']) {
            let id = endpoint[..gt].trim().to_string();
            let inner = &endpoint[gt + 1..endpoint.len() - 1];
            return (id, NodeShape::Odd, Some(unquote(inner).to_string()), class);
        }
    }
    // Find where the shape bracket (if any) begins.
    let open = endpoint.find(['[', '(', '{']);
    let Some(open) = open else {
        return (endpoint.to_string(), NodeShape::Rectangle, None, class);
    };

    let id = endpoint[..open].trim().to_string();
    let rest = &endpoint[open..];
    // Two-character delimiters are checked before single-character ones. The
    // `[/…/]`, `[/…\]`, `[\…\]`, `[\…/]` forms share an opener, so the closer
    // disambiguates.
    let strip = |pre: &str, suf: &str| rest.strip_prefix(pre).and_then(|r| r.strip_suffix(suf));
    let (shape, inner) = if let Some(inner) = strip("((", "))") {
        (NodeShape::Circle, inner)
    } else if let Some(inner) = strip("{{", "}}") {
        (NodeShape::Hexagon, inner)
    } else if let Some(inner) = strip("([", "])") {
        (NodeShape::Stadium, inner)
    } else if let Some(inner) = strip("[[", "]]") {
        (NodeShape::Subroutine, inner)
    } else if let Some(inner) = strip("[(", ")]") {
        (NodeShape::Cylinder, inner)
    } else if let Some(inner) = strip("[/", "/]") {
        (NodeShape::Parallelogram, inner)
    } else if let Some(inner) = strip("[/", "\\]") {
        (NodeShape::Trapezoid, inner)
    } else if let Some(inner) = strip("[\\", "\\]") {
        (NodeShape::LeanLeft, inner)
    } else if let Some(inner) = strip("[\\", "/]") {
        (NodeShape::InvTrapezoid, inner)
    } else if let Some(inner) = strip("[", "]") {
        (NodeShape::Rectangle, inner)
    } else if let Some(inner) = strip("(", ")") {
        (NodeShape::RoundedRectangle, inner)
    } else if let Some(inner) = strip("{", "}") {
        (NodeShape::Rhombus, inner)
    } else {
        // Unbalanced/unknown bracketing — treat the whole token as an id.
        return (endpoint.to_string(), NodeShape::Rectangle, None, class);
    };

    (id, shape, Some(unquote(inner).to_string()), class)
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
        assert_eq!(chart.edges[0].arrow_end, ArrowType::Point);
    }

    #[test]
    fn parses_edge_label_and_chain() {
        let chart = parse("flowchart TD\n A --> B -->|next| C").unwrap();
        assert_eq!(chart.nodes.len(), 3);
        assert_eq!(chart.edges.len(), 2);
        assert_eq!(chart.edges[1].label.as_deref(), Some("next"));
    }

    #[test]
    fn multiline_quoted_label_is_one_node() {
        // A label spanning several physical lines must be one node, not several.
        let chart = parse("flowchart LR\n a[\"\nline one\nline two\n\"] --> b").unwrap();
        assert_eq!(chart.nodes.len(), 2);
        assert_eq!(chart.nodes[0].id, "a");
        assert!(chart.nodes[0].label.contains("line one") && chart.nodes[0].label.contains("line two"));
    }

    #[test]
    fn captures_acc_and_frontmatter_title() {
        // accTitle/accDescr captured; frontmatter title set by the dispatcher.
        let d = crate::parser::parse(
            "---\ntitle: My Chart\n---\nflowchart LR\n accTitle: Acc T\n accDescr: Acc D\n A --> B",
        )
        .unwrap();
        let crate::ir::Diagram::Flowchart(f) = d;
        assert_eq!(f.title.as_deref(), Some("My Chart"));
        assert_eq!(f.acc_title.as_deref(), Some("Acc T"));
        assert_eq!(f.acc_descr.as_deref(), Some("Acc D"));
        assert_eq!(f.nodes.len(), 2);
    }

    #[test]
    fn skips_accessibility_metadata() {
        let chart = parse(
            "flowchart LR\n accTitle: A title\n accDescr: A description\n A --> B",
        )
        .unwrap();
        let ids: Vec<_> = chart.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["A", "B"]);
    }

    #[test]
    fn parses_v11_at_metadata_shape_and_label() {
        let chart = parse(
            "flowchart LR\n DataStore@{shape: datastore, label: \"Datastore\"} --> B@{shape: circle}",
        )
        .unwrap();
        assert_eq!(chart.nodes[0].id, "DataStore");
        assert_eq!(chart.nodes[0].label, "Datastore");
        assert_eq!(chart.nodes[0].shape, NodeShape::DataStore);
        assert_eq!(chart.nodes[1].shape, NodeShape::Circle);
    }

    #[test]
    fn parses_polygon_shapes() {
        use NodeShape::*;
        let chart = parse(
            "flowchart TD\n a{{H}} --> b[[S]]\n b --> c[/P/]\n c --> d[/T\\]\n d --> e[\\L\\]\n e --> f[\\I/]\n f --> g[(C)]",
        )
        .unwrap();
        let shapes: Vec<_> = chart.nodes.iter().map(|n| (n.id.as_str(), n.shape)).collect();
        assert_eq!(
            shapes,
            [
                ("a", Hexagon), ("b", Subroutine), ("c", Parallelogram),
                ("d", Trapezoid), ("e", LeanLeft), ("f", InvTrapezoid), ("g", Cylinder),
            ]
        );
        // Labels are extracted without the delimiters.
        assert_eq!(chart.nodes[0].label, "H");
        assert_eq!(chart.nodes[6].label, "C");
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
