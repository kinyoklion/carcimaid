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
    let source = strip_frontmatter(source);
    let header = first_keyword(source)
        .ok_or_else(|| Error::Parse("empty diagram (no content)".into()))?;

    match header {
        "flowchart" | "graph" => Ok(Diagram::Flowchart(flowchart::parse(source)?)),
        other => Err(Error::Unsupported(format!("diagram type `{other}`"))),
    }
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
