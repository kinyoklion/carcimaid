//! Runs the official mermaid CLI (`mmdc`) as the reference renderer.
//!
//! The mermaid CLI needs a headless browser, so the most reproducible way to
//! run it on NixOS is via the official Docker image (`minlag/mermaid-cli`),
//! which bundles Chromium. This module shells out to `docker run`.

use std::fs;
use std::path::Path;
use std::process::Command;

/// mermaid CLI config passed via `-c`. `htmlLabels: false` makes mermaid emit
/// SVG `<text>` labels instead of `foreignObject` HTML (which needs a browser
/// to lay out and is impossible to reproduce in pure Rust). `useMaxWidth:
/// false` makes the root `<svg>` carry numeric `width`/`height` instead of
/// `100%`, which the structural comparator can diff numerically. Both choices
/// are mirrored by carcimaid's renderer so the two outputs are comparable.
///
/// `useMaxWidth` is a per-diagram config key (each type extends mermaid's
/// `BaseDiagramConfig`), so it is set for every diagram type in the corpus —
/// mermaid merges config leniently, so keys for types that ignore it are
/// harmless. `htmlLabels: false` is set for the label-bearing graph types.
const MERMAID_CONFIG: &str = r#"{
  "htmlLabels": false,
  "securityLevel": "loose",
  "flowchart": { "htmlLabels": false, "useMaxWidth": false },
  "sequence": { "useMaxWidth": false },
  "class": { "htmlLabels": false, "useMaxWidth": false },
  "state": { "htmlLabels": false, "useMaxWidth": false },
  "er": { "useMaxWidth": false },
  "gantt": { "useMaxWidth": false },
  "journey": { "useMaxWidth": false },
  "timeline": { "useMaxWidth": false },
  "pie": { "useMaxWidth": false },
  "quadrantChart": { "useMaxWidth": false },
  "xyChart": { "useMaxWidth": false },
  "requirement": { "useMaxWidth": false },
  "architecture": { "useMaxWidth": false },
  "mindmap": { "useMaxWidth": false },
  "kanban": { "useMaxWidth": false },
  "gitGraph": { "useMaxWidth": false },
  "c4": { "useMaxWidth": false },
  "sankey": { "useMaxWidth": false },
  "packet": { "useMaxWidth": false },
  "block": { "useMaxWidth": false },
  "radar": { "useMaxWidth": false }
}"#;

/// Configuration for invoking the oracle.
#[derive(Debug, Clone)]
pub struct Oracle {
    /// Container runtime binary (`docker` or `podman`).
    pub runtime: String,
    /// Fully-qualified image reference.
    pub image: String,
}

impl Default for Oracle {
    fn default() -> Self {
        Oracle {
            runtime: "docker".to_string(),
            // Fully qualified so podman's short-name resolution does not fail.
            image: "docker.io/minlag/mermaid-cli:latest".to_string(),
        }
    }
}

/// Errors from running the oracle.
#[derive(Debug)]
pub enum OracleError {
    Io(std::io::Error),
    /// `mmdc` exited non-zero. Carries combined stdout+stderr.
    Render(String),
}

impl std::fmt::Display for OracleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OracleError::Io(e) => write!(f, "io error invoking oracle: {e}"),
            OracleError::Render(m) => write!(f, "mermaid-cli failed: {m}"),
        }
    }
}

impl std::error::Error for OracleError {}

impl From<std::io::Error> for OracleError {
    fn from(e: std::io::Error) -> Self {
        OracleError::Io(e)
    }
}

impl Oracle {
    /// Render `source` to an SVG string using the mermaid CLI.
    ///
    /// `workdir` is a writable directory that gets bind-mounted into the
    /// container; the diagram and its output live there. Caller owns cleanup.
    pub fn render(&self, source: &str, workdir: &Path) -> Result<String, OracleError> {
        fs::create_dir_all(workdir)?;
        let input = workdir.join("input.mmd");
        let output = workdir.join("output.svg");
        let config = workdir.join("config.json");
        fs::write(&input, source)?;
        fs::write(&config, MERMAID_CONFIG)?;
        // Remove any stale output so a silent failure can't masquerade as success.
        let _ = fs::remove_file(&output);

        // Bind mounts must be absolute paths, otherwise podman/docker treats
        // the source as a (named) volume.
        let abs = workdir.canonicalize()?;
        let mount = format!("{}:/data", abs.display());
        // `--user 0:0` makes the container run as container-root, which maps to
        // the single available host UID under rootless podman. Without it the
        // image's default USER (1001) triggers `setresgid: Invalid argument`.
        let out = Command::new(&self.runtime)
            .args(["run", "--rm", "--user", "0:0", "-v", &mount, &self.image])
            .args(["-c", "/data/config.json"])
            .args(["-i", "/data/input.mmd", "-o", "/data/output.svg"])
            .output()?;

        if !out.status.success() || !output.exists() {
            let mut msg = String::from_utf8_lossy(&out.stdout).into_owned();
            msg.push_str(&String::from_utf8_lossy(&out.stderr));
            return Err(OracleError::Render(msg));
        }
        Ok(fs::read_to_string(&output)?)
    }

    /// Check the runtime and image are usable by running `--version`.
    pub fn is_available(&self) -> bool {
        Command::new(&self.runtime)
            .args(["image", "inspect", &self.image])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
