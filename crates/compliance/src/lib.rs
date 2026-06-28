//! Compliance harness library.
//!
//! Strategy: for every diagram in the corpus, render it two ways — with the
//! official mermaid CLI ([`oracle`], the reference) and with [`carcimaid`] — and
//! compare the two SVGs **structurally** ([`svgdiff`]). The gap between them is
//! the development signal: we drive carcimaid's parser/layout/render until the
//! structural diff shrinks to within tolerance.

pub mod corpus;
pub mod oracle;
pub mod svgdiff;
