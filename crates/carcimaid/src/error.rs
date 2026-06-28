//! Error types shared across the rendering pipeline.

use std::fmt;

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur while rendering a mermaid diagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The source could not be parsed into a known diagram.
    Parse(String),
    /// The diagram type is recognised but not yet supported.
    Unsupported(String),
    /// Layout failed (e.g. an inconsistent graph).
    Layout(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse(m) => write!(f, "parse error: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
            Error::Layout(m) => write!(f, "layout error: {m}"),
        }
    }
}

impl std::error::Error for Error {}
