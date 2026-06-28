//! Discovery of corpus diagrams on disk.
//!
//! The corpus lives under `corpus/` at the repository root, organised by
//! diagram type (e.g. `corpus/flowchart/*.mmd`). Each `.mmd` file is one test
//! case. Provenance/attribution for the corpus is recorded in
//! `corpus/ATTRIBUTION.md`.

use std::fs;
use std::path::{Path, PathBuf};

/// One corpus diagram.
#[derive(Debug, Clone)]
pub struct Case {
    /// Stable identifier derived from the path relative to the corpus root,
    /// e.g. `flowchart/simple-decision`.
    pub id: String,
    pub path: PathBuf,
    pub source: String,
}

/// Recursively collect all `.mmd` files under `root`, sorted by id for
/// deterministic ordering.
pub fn discover(root: &Path) -> std::io::Result<Vec<Case>> {
    let mut cases = Vec::new();
    collect(root, root, &mut cases)?;
    cases.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(cases)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<Case>) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("mmd") {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let id = rel.with_extension("").to_string_lossy().replace('\\', "/");
            let source = fs::read_to_string(&path)?;
            out.push(Case { id, path, source });
        }
    }
    Ok(())
}
