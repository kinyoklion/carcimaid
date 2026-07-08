//! Parsing mermaid source text into the [`crate::ir`] diagram model.
//!
//! The top-level [`parse`] sniffs the diagram type from the first significant
//! line and dispatches to a per-type parser. Only flowcharts are implemented so
//! far; other types return [`Error::Unsupported`].

use crate::ir::{Diagram, Look, Theme};
use crate::{Error, Result};

pub mod flowchart;
pub mod sequence;

/// Parse mermaid source into a [`Diagram`].
pub fn parse(source: &str) -> Result<Diagram> {
    let title = frontmatter_title(source);
    let node_spacing = frontmatter_flowchart_num(source, "nodeSpacing");
    let rank_spacing = frontmatter_flowchart_num(source, "rankSpacing");
    let look = frontmatter_look(source).or_else(|| init_look(source));
    let theme = frontmatter_theme(source);
    let source = strip_frontmatter(source);
    let header =
        first_keyword(source).ok_or_else(|| Error::Parse("empty diagram (no content)".into()))?;

    match header {
        "flowchart" | "graph" => {
            let mut f = flowchart::parse(source)?;
            f.title = title; // visible title from frontmatter
            f.node_spacing = node_spacing;
            f.rank_spacing = rank_spacing;
            if let Some(lk) = look {
                f.look = lk;
            }
            if let Some(th) = theme {
                f.theme = th;
            }
            Ok(Diagram::Flowchart(f))
        }
        "sequenceDiagram" => {
            let mut s = sequence::parse(source)?;
            if s.title.is_none() {
                s.title = title; // visible title from frontmatter
            }
            Ok(Diagram::Sequence(s))
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

/// Read the top-level `config.look` from the frontmatter, mapping mermaid's
/// look names to a [`Look`]. `look:` is a sibling of `flowchart:` inside the
/// `config:` block; we don't need to know its exact nesting — any `look:` line
/// in the frontmatter is the diagram look (the `flowchart:` sub-map has no such
/// key), so a lightweight scan for the first `look:` line suffices.
fn frontmatter_look(source: &str) -> Option<Look> {
    let mut lines = source.lines().skip_while(|l| l.trim().is_empty());
    if lines.next()?.trim() != "---" {
        return None;
    }
    for l in lines {
        let t = l.trim();
        if t == "---" {
            break;
        }
        if let Some(v) = t.strip_prefix("look:") {
            return look_from_str(v.trim().trim_matches(['"', '\'']));
        }
    }
    None
}

/// Read the top-level `config.theme` from the frontmatter, mapping mermaid's
/// theme names to a [`Theme`]. Like `look`, `theme:` is a sibling of
/// `flowchart:` inside `config:`; the `flowchart:` sub-map has no `theme:` key,
/// so a lightweight scan for the first `theme:` line in the frontmatter block
/// suffices. Returns `None` for unknown/absent themes (renderer uses default).
fn frontmatter_theme(source: &str) -> Option<Theme> {
    let mut lines = source.lines().skip_while(|l| l.trim().is_empty());
    if lines.next()?.trim() != "---" {
        return None;
    }
    for l in lines {
        let t = l.trim();
        if t == "---" {
            break;
        }
        if let Some(v) = t.strip_prefix("theme:") {
            return theme_from_str(v.trim().trim_matches(['"', '\'']));
        }
    }
    None
}

/// Map a mermaid `theme` config value to a [`Theme`].
fn theme_from_str(v: &str) -> Option<Theme> {
    match v {
        "default" => Some(Theme::Default),
        "base" => Some(Theme::Base),
        "forest" => Some(Theme::Forest),
        "dark" => Some(Theme::Dark),
        "neutral" => Some(Theme::Neutral),
        _ => None,
    }
}

/// Read `look` from a `%%{init: {"look":"handDrawn"}}%%` directive line, if any.
/// This is a best-effort substring scan rather than a JSON parse.
fn init_look(source: &str) -> Option<Look> {
    for l in source.lines() {
        let t = l.trim();
        if t.starts_with("%%{") && t.contains("look") {
            if t.contains("handDrawn") {
                return Some(Look::HandDrawn);
            }
            if t.contains("classic") || t.contains("neo") {
                return Some(Look::Classic);
            }
        }
    }
    None
}

/// Map a mermaid `look` config value to a [`Look`] (`neo` is treated as the
/// clean/classic rendering, since we don't have a distinct neo look yet).
fn look_from_str(v: &str) -> Option<Look> {
    match v {
        "handDrawn" => Some(Look::HandDrawn),
        "classic" | "neo" => Some(Look::Classic),
        _ => None,
    }
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

/// The first whitespace-delimited keyword of the first non-blank, non-comment
/// line — used to identify the diagram type.
fn first_keyword(source: &str) -> Option<&str> {
    source
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .and_then(|l| l.split_whitespace().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_frontmatter_then_parses() {
        let src = "---\ntitle: My Chart\nconfig:\n  flowchart:\n    curve: linear\n---\nflowchart TD\n A --> B";
        let diagram = parse(src).unwrap();
        let Diagram::Flowchart(f) = diagram else {
            unreachable!()
        };
        assert_eq!(f.nodes.len(), 2);
    }

    #[test]
    fn parses_handdrawn_look() {
        let src = "---\nconfig:\n  look: handDrawn\n---\nflowchart TD\n A --> B";
        let Diagram::Flowchart(f) = parse(src).unwrap() else {
            unreachable!()
        };
        assert_eq!(f.look, Look::HandDrawn);
    }

    #[test]
    fn defaults_to_classic_look() {
        let Diagram::Flowchart(f) = parse("flowchart TD\n A --> B").unwrap() else {
            unreachable!()
        };
        assert_eq!(f.look, Look::Classic);
    }

    #[test]
    fn parses_theme_forest() {
        let src = "---\nconfig:\n  theme: forest\n---\nflowchart TD\n A --> B";
        let Diagram::Flowchart(f) = parse(src).unwrap() else {
            unreachable!()
        };
        assert_eq!(f.theme, Theme::Forest);
    }

    #[test]
    fn parses_theme_base_with_other_config() {
        let src = "---\ntitle: T\nconfig:\n  theme: base\n  flowchart:\n    curve: cardinal\n---\nflowchart LR\n A --> B";
        let Diagram::Flowchart(f) = parse(src).unwrap() else {
            unreachable!()
        };
        assert_eq!(f.theme, Theme::Base);
    }

    #[test]
    fn defaults_to_default_theme() {
        let Diagram::Flowchart(f) = parse("flowchart TD\n A --> B").unwrap() else {
            unreachable!()
        };
        assert_eq!(f.theme, Theme::Default);
    }

    #[test]
    fn no_frontmatter_is_unchanged() {
        assert_eq!(
            strip_frontmatter("flowchart TD\n A --> B").lines().next(),
            Some("flowchart TD")
        );
    }
}
