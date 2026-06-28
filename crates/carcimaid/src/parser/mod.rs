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
    let header = first_keyword(source)
        .ok_or_else(|| Error::Parse("empty diagram (no content)".into()))?;

    match header {
        "flowchart" | "graph" => Ok(Diagram::Flowchart(flowchart::parse(source)?)),
        other => Err(Error::Unsupported(format!("diagram type `{other}`"))),
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
