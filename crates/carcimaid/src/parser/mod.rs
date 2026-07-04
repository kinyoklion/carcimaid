//! Parsing mermaid source text into the [`crate::ir`] diagram model.
//!
//! The top-level [`parse`] sniffs the diagram type from the first significant
//! line and dispatches to a per-type parser. Only flowcharts are implemented so
//! far; other types return [`Error::Unsupported`].

use crate::ir::Diagram;
use crate::{Error, Result};

pub mod flowchart;

/// Parse mermaid source into a [`Diagram`].
pub fn parse(source: &str) -> Result<Diagram> {
    let title = frontmatter_title(source);
    let node_spacing = frontmatter_flowchart_num(source, "nodeSpacing");
    let rank_spacing = frontmatter_flowchart_num(source, "rankSpacing");
    let source = strip_frontmatter(source);
    let header = first_keyword(source)
        .ok_or_else(|| Error::Parse("empty diagram (no content)".into()))?;

    match header {
        "flowchart" | "graph" => {
            let mut f = flowchart::parse(source)?;
            f.title = title; // visible title from frontmatter
            f.node_spacing = node_spacing;
            f.rank_spacing = rank_spacing;
            Ok(Diagram::Flowchart(f))
        }
        other => Err(Error::Unsupported(format!("diagram type `{other}`"))),
    }
}

/// Read a numeric value from the frontmatter `config.flowchart.<key>` block,
/// e.g. `nodeSpacing`/`rankSpacing`. We only need the `flowchart:` sub-map, so
/// this is a lightweight scan rather than a full YAML parse: find the
/// `flowchart:` line inside `config:`, then the first `  <key>: <num>` under it.
fn frontmatter_flowchart_num(source: &str, key: &str) -> Option<f64> {
    let mut lines = source.lines().skip_while(|l| l.trim().is_empty());
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut in_flowchart = false;
    for l in lines {
        let t = l.trim();
        if t == "---" {
            break;
        }
        // `flowchart:` opens the sub-map; any other top-level `config`/`x:` key
        // (no leading indent beyond the config block) closes it.
        if t == "flowchart:" {
            in_flowchart = true;
            continue;
        }
        if in_flowchart {
            if let Some(v) = t.strip_prefix(key).and_then(|r| r.trim().strip_prefix(':')) {
                return v.trim().parse().ok();
            }
        }
    }
    None
}

/// Extract the `title:` from a leading YAML frontmatter block, if present.
fn frontmatter_title(source: &str) -> Option<String> {
    let mut lines = source.lines().skip_while(|l| l.trim().is_empty());
    if lines.next()?.trim() != "---" {
        return None;
    }
    for l in lines {
        if l.trim() == "---" {
            break;
        }
        if let Some(t) = l.trim().strip_prefix("title:") {
            return Some(t.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// Strip a leading YAML frontmatter block (`---` … `---`), which mermaid uses
/// for diagram `title`/`config`. The fences may carry surrounding whitespace.
/// Returns the source unchanged if no opening fence is present.
fn strip_frontmatter(source: &str) -> &str {
    let mut lines = source.lines();
    // Opening fence: the first non-blank line must be exactly `---`.
    let mut consumed = 0;
    loop {
        match lines.next() {
            Some(l) if l.trim().is_empty() => consumed += l.len() + 1,
            Some(l) if l.trim() == "---" => {
                consumed += l.len() + 1;
                break;
            }
            _ => return source,
        }
    }
    // Closing fence: the next line that is exactly `---`.
    for l in lines {
        consumed += l.len() + 1;
        if l.trim() == "---" {
            return source.get(consumed..).unwrap_or("");
        }
    }
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_frontmatter_then_parses() {
        let src = "---\ntitle: My Chart\nconfig:\n  flowchart:\n    curve: linear\n---\nflowchart TD\n A --> B";
        let diagram = parse(src).unwrap();
        let Diagram::Flowchart(f) = diagram;
        assert_eq!(f.nodes.len(), 2);
    }

    #[test]
    fn no_frontmatter_is_unchanged() {
        assert_eq!(strip_frontmatter("flowchart TD\n A --> B").lines().next(), Some("flowchart TD"));
    }
}

/// The first whitespace-delimited keyword of the first non-blank, non-comment
/// line — used to identify the diagram type.
fn first_keyword(source: &str) -> Option<&str> {
    source
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .and_then(|l| l.split_whitespace().next())
}
