//! A pragmatic line-oriented parser for mermaid `sequenceDiagram` source.
//!
//! Like the flowchart parser this is a hand-written subset that grows with the
//! corpus. It handles participant/actor declarations (with `as` aliases),
//! messages (all arrow operators, with `+`/`-` activation suffixes), notes,
//! `activate`/`deactivate`, `autonumber`, `title`, and the block constructs
//! (`loop`/`alt`/`else`/`opt`/`par`/`and`/`critical`/`break`/`rect` … `end`).
//! Participants referenced by a message before declaration are auto-created,
//! matching mermaid.

use crate::ir::{
    BlockBoundary, NotePlacement, Participant, SeqArrow, SeqEvent, SeqMessage, SeqNote,
    SequenceDiagram,
};
use crate::Result;

/// Parse a sequence diagram from full source (including its `sequenceDiagram`
/// header line).
pub fn parse(source: &str) -> Result<SequenceDiagram> {
    let mut d = SequenceDiagram::default();
    let mut header_seen = false;

    // The `box` currently open (participants declared inside join it).
    let mut cur_box: Option<usize> = None;
    for raw in split_statements(source) {
        let stmt = raw.trim();
        if stmt.is_empty() {
            continue;
        }
        if !header_seen {
            header_seen = true;
            if stmt.split_whitespace().next() == Some("sequenceDiagram") {
                let rest = stmt["sequenceDiagram".len()..].trim();
                if rest.is_empty() {
                    continue;
                }
                parse_statement(rest, &mut d, &mut cur_box);
                continue;
            }
        }
        parse_statement(stmt, &mut d, &mut cur_box);
    }
    Ok(d)
}

/// Split source into statements on newlines and `;`, dropping `%%` comments.
/// Sequence syntax has no bracket-nested newlines to protect, so this is a
/// straightforward scan.
fn split_statements(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        // Strip a `%%` line comment (sequence has no inline `%%{}` directives
        // we need mid-line, so cut at the first `%%`).
        let line = match line.find("%%") {
            Some(i) => &line[..i],
            None => line,
        };
        // Sequence statements are newline-terminated (unlike flowchart, `;` is
        // not a separator — splitting on it would break `#lt;`-style entity
        // escapes and any label containing a semicolon).
        out.push(line.to_string());
    }
    out
}

/// Parse one statement into the diagram, dispatching on its leading keyword.
fn parse_statement(stmt: &str, d: &mut SequenceDiagram, cur_box: &mut Option<usize>) {
    // accessibility metadata
    if let Some(rest) = stmt.strip_prefix("accTitle") {
        d.acc_title = Some(rest.trim().trim_start_matches(':').trim().to_string());
        return;
    }
    if let Some(rest) = stmt.strip_prefix("accDescr") {
        d.acc_descr = Some(rest.trim().trim_start_matches(':').trim().to_string());
        return;
    }
    // `title: text` or `title text`
    if let Some(rest) = keyword(stmt, "title") {
        d.title = Some(rest.trim_start_matches(':').trim().to_string());
        return;
    }
    // `box [color] name` — open a participant grouping.
    if let Some(rest) = keyword(stmt, "box") {
        let (color, name) = parse_box_header(rest.trim());
        d.boxes.push(crate::ir::SeqBox { name, color });
        *cur_box = Some(d.boxes.len() - 1);
        return;
    }
    // `end` closes an open box (else it's a block boundary, handled below).
    if stmt == "end" && cur_box.is_some() {
        *cur_box = None;
        return;
    }
    // participant / actor declarations
    if let Some(rest) = keyword(stmt, "participant") {
        declare_participant(rest, false, d, *cur_box);
        return;
    }
    if let Some(rest) = keyword(stmt, "actor") {
        declare_participant(rest, true, d, *cur_box);
        return;
    }
    if let Some(rest) = keyword(stmt, "create") {
        // `create participant X` / `create actor X` / `create X`
        let idx = if let Some(r) = keyword(rest, "participant") {
            declare_participant(r, false, d, *cur_box)
        } else if let Some(r) = keyword(rest, "actor") {
            declare_participant(r, true, d, *cur_box)
        } else {
            declare_participant(rest, false, d, *cur_box)
        };
        if let Some(i) = idx {
            d.events.push(SeqEvent::Create(i));
        }
        return;
    }
    if let Some(rest) = keyword(stmt, "destroy") {
        let i = ensure_participant(rest.trim(), false, d);
        d.events.push(SeqEvent::Destroy(i));
        return;
    }
    // activation
    if let Some(rest) = keyword(stmt, "activate") {
        let i = ensure_participant(rest.trim(), false, d);
        d.events.push(SeqEvent::Activate(i));
        return;
    }
    if let Some(rest) = keyword(stmt, "deactivate") {
        let i = ensure_participant(rest.trim(), false, d);
        d.events.push(SeqEvent::Deactivate(i));
        return;
    }
    // autonumber
    if stmt == "autonumber" || keyword(stmt, "autonumber").is_some() {
        let rest = keyword(stmt, "autonumber").unwrap_or("").trim();
        if rest == "off" || rest.is_empty() {
            d.events
                .push(SeqEvent::Autonumber(if rest == "off" { None } else { Some((1, 1)) }));
        } else {
            let mut it = rest.split_whitespace();
            let start = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
            let step = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
            d.events.push(SeqEvent::Autonumber(Some((start, step))));
        }
        return;
    }
    // notes
    if let Some(rest) = keyword(stmt, "note").or_else(|| keyword(stmt, "Note")) {
        parse_note(rest, d);
        return;
    }
    // block constructs
    if let Some(b) = parse_block_boundary(stmt, d) {
        d.events.push(SeqEvent::Block(b));
        return;
    }
    // Links/properties: recognised but not modelled yet (skipped so they don't
    // become phantom messages).
    if keyword(stmt, "link").is_some()
        || keyword(stmt, "links").is_some()
        || keyword(stmt, "properties").is_some()
        || keyword(stmt, "details").is_some()
    {
        return;
    }
    // Otherwise: a message.
    parse_message(stmt, d);
}

/// A block-boundary keyword (`loop`/`alt`/`else`/…/`end`), if this statement is
/// one. `rect <color>` opens a coloured region.
fn parse_block_boundary(stmt: &str, _d: &mut SequenceDiagram) -> Option<BlockBoundary> {
    if stmt == "end" {
        // Closing keyword; the specific end variant is resolved at layout time
        // by matching the most recent open block. We emit a generic end per
        // construct instead — see below for the paired starts.
        return Some(BlockBoundary::LoopEnd);
    }
    if let Some(r) = keyword(stmt, "loop") {
        return Some(BlockBoundary::LoopStart(r.trim().to_string()));
    }
    if let Some(r) = keyword(stmt, "alt") {
        return Some(BlockBoundary::AltStart(r.trim().to_string()));
    }
    if let Some(r) = keyword(stmt, "else") {
        return Some(BlockBoundary::AltElse(r.trim().to_string()));
    }
    if let Some(r) = keyword(stmt, "opt") {
        return Some(BlockBoundary::OptStart(r.trim().to_string()));
    }
    if let Some(r) = keyword(stmt, "par") {
        return Some(BlockBoundary::ParStart(r.trim().to_string()));
    }
    if let Some(r) = keyword(stmt, "and") {
        return Some(BlockBoundary::ParAnd(r.trim().to_string()));
    }
    if let Some(r) = keyword(stmt, "rect") {
        return Some(BlockBoundary::RectStart(r.trim().to_string()));
    }
    None
}

/// Parse a `participant`/`actor` declaration body: `X` or `X as Alias`.
/// `box_idx` is the enclosing `box` grouping, if any. Returns the participant
/// index (or `None` for an empty body).
fn declare_participant(
    body: &str,
    is_actor: bool,
    d: &mut SequenceDiagram,
    box_idx: Option<usize>,
) -> Option<usize> {
    let body = body.trim();
    // Strip a trailing `@{ ... }` metadata block (participant shapes).
    let body = body.split('@').next().unwrap_or(body).trim();
    let (id, label) = match split_as(body) {
        Some((id, alias)) => (id.trim().to_string(), alias.trim().to_string()),
        None => (body.to_string(), body.to_string()),
    };
    if id.is_empty() {
        return None;
    }
    match d.participant_index(&id) {
        Some(i) => {
            // Re-declaration updates label/kind.
            d.participants[i].label = label;
            d.participants[i].is_actor = is_actor;
            if box_idx.is_some() {
                d.participants[i].box_idx = box_idx;
            }
            Some(i)
        }
        None => {
            d.participants.push(Participant { id, label, is_actor, box_idx });
            Some(d.participants.len() - 1)
        }
    }
}

/// Ensure a participant with id `id` exists (auto-created on first message
/// reference), returning its index.
fn ensure_participant(id: &str, is_actor: bool, d: &mut SequenceDiagram) -> usize {
    let id = id.trim();
    if let Some(i) = d.participant_index(id) {
        return i;
    }
    d.participants.push(Participant {
        id: id.to_string(),
        label: id.to_string(),
        is_actor,
        box_idx: None,
    });
    d.participants.len() - 1
}

/// Parse a `box` header: an optional leading colour (`rgb(...)`, `#hex`, or a
/// CSS colour name) followed by the box name.
fn parse_box_header(rest: &str) -> (Option<String>, String) {
    let first = rest.split_whitespace().next().unwrap_or("");
    let is_color = first.starts_with("rgb(")
        || first.starts_with("rgba(")
        || first.starts_with('#')
        || is_color_name(first);
    if is_color && !first.is_empty() {
        // rgb(...) may contain spaces; split at the closing paren if present.
        if let Some(close) = rest.find(')') {
            if rest[..close].contains('(') {
                let color = rest[..=close].to_string();
                return (Some(color), rest[close + 1..].trim().to_string());
            }
        }
        let name = rest[first.len()..].trim().to_string();
        (Some(first.to_string()), name)
    } else {
        (None, rest.to_string())
    }
}

/// A small set of CSS colour names used by `box`/`rect` in the corpus (plus
/// common ones). Not exhaustive — unknown words are treated as box names.
fn is_color_name(w: &str) -> bool {
    matches!(
        w.to_ascii_lowercase().as_str(),
        "transparent" | "red" | "green" | "blue" | "purple" | "yellow" | "orange" | "pink"
            | "cyan" | "magenta" | "black" | "white" | "gray" | "grey" | "lightgreen"
            | "lightblue" | "lightgrey" | "lightgray" | "lightyellow" | "lightpink"
            | "aqua" | "teal" | "navy" | "olive" | "maroon" | "silver" | "gold" | "coral"
    )
}

/// Parse a `note` statement: `left of X: t` / `right of X: t` / `over X[,Y]: t`.
fn parse_note(rest: &str, d: &mut SequenceDiagram) {
    let rest = rest.trim();
    let (placement, after) = if let Some(r) = keyword(rest, "left of") {
        (NotePlacement::LeftOf, r)
    } else if let Some(r) = keyword(rest, "right of") {
        (NotePlacement::RightOf, r)
    } else if let Some(r) = keyword(rest, "over") {
        (NotePlacement::Over, r)
    } else {
        return;
    };
    let (actors_str, mut text) = match after.split_once(':') {
        Some((a, t)) => (a.trim(), t.trim().to_string()),
        None => (after.trim(), String::new()),
    };
    // A leading `wrap:` / `nowrap:` is a directive (from `Note over X:wrap: …`),
    // not part of the text.
    let mut wrap = false;
    if let Some(t) = text.strip_prefix("wrap:") {
        wrap = true;
        text = t.trim().to_string();
    } else if let Some(t) = text.strip_prefix("nowrap:") {
        text = t.trim().to_string();
    }
    let actors: Vec<usize> = actors_str
        .split(',')
        .map(|a| ensure_participant(a.trim(), false, d))
        .collect();
    if actors.is_empty() {
        return;
    }
    d.events.push(SeqEvent::Note(SeqNote { placement, actors, text, wrap }));
}

/// Parse a message statement: `LHS <arrow>[+/-] RHS : text`.
fn parse_message(stmt: &str, d: &mut SequenceDiagram) {
    let Some((op_start, op_end, arrow)) = find_arrow(stmt) else {
        return; // not a message we recognise; skip
    };
    let from_str = stmt[..op_start].trim();
    let mut rhs = &stmt[op_end..];

    // `+`/`-` activation suffix directly after the arrow, before the target.
    let (mut activate, mut deactivate) = (false, false);
    let rhs_trimmed = rhs.trim_start();
    if let Some(r) = rhs_trimmed.strip_prefix('+') {
        activate = true;
        rhs = r;
    } else if let Some(r) = rhs_trimmed.strip_prefix('-') {
        deactivate = true;
        rhs = r;
    }

    let (to_str, mut text) = match rhs.split_once(':') {
        Some((t, msg)) => (t.trim(), msg.trim().to_string()),
        None => (rhs.trim(), String::new()),
    };
    // A leading `wrap:` / `nowrap:` is a directive, not part of the label.
    let mut wrap = false;
    if let Some(t) = text.strip_prefix("wrap:") {
        wrap = true;
        text = t.trim().to_string();
    } else if let Some(t) = text.strip_prefix("nowrap:") {
        text = t.trim().to_string();
    }
    if from_str.is_empty() || to_str.is_empty() {
        return;
    }
    let from = ensure_participant(from_str, false, d);
    let to = ensure_participant(to_str, false, d);
    d.events.push(SeqEvent::Message(SeqMessage {
        from,
        to,
        text,
        arrow,
        activate,
        deactivate,
        wrap,
    }));
}

/// Locate the message arrow operator in `stmt`, returning
/// `(start, end, arrow)` byte offsets and the parsed [`SeqArrow`]. Operators
/// are matched longest-first at the earliest position (sequence ids cannot
/// contain `-`/`<`/`x`/`)`, so the first hit is unambiguous).
fn find_arrow(stmt: &str) -> Option<(usize, usize, SeqArrow)> {
    // (token, arrow) longest-first so a prefix (`->`) never shadows `->>`.
    const OPS: &[(&str, SeqArrow)] = &[
        ("<<-->>", SeqArrow::BiDotted),
        ("<<->>", SeqArrow::BiSolid),
        // Directional (solid-triangle) arrows, dotted forms first (longer).
        ("--|\\", SeqArrow::SolidTopDotted),
        ("--|/", SeqArrow::SolidBottomDotted),
        ("/|--", SeqArrow::SolidTopRevDotted),
        ("\\|--", SeqArrow::SolidBottomRevDotted),
        ("--\\\\", SeqArrow::StickTopDotted),
        ("--//", SeqArrow::StickBottomDotted),
        ("//--", SeqArrow::StickTopRevDotted),
        ("\\\\--", SeqArrow::StickBottomRevDotted),
        ("-|\\", SeqArrow::SolidTop),
        ("-|/", SeqArrow::SolidBottom),
        ("/|-", SeqArrow::SolidTopRev),
        ("\\|-", SeqArrow::SolidBottomRev),
        ("-\\\\", SeqArrow::StickTop),
        ("-//", SeqArrow::StickBottom),
        ("//-", SeqArrow::StickTopRev),
        ("\\\\-", SeqArrow::StickBottomRev),
        ("-->>", SeqArrow::DottedArrow),
        ("->>", SeqArrow::SolidArrow),
        ("--x", SeqArrow::DottedCross),
        ("--)", SeqArrow::DottedPoint),
        ("-->", SeqArrow::DottedOpen),
        ("-x", SeqArrow::SolidCross),
        ("-)", SeqArrow::SolidPoint),
        ("->", SeqArrow::SolidOpen),
    ];
    let mut best: Option<(usize, usize, SeqArrow)> = None;
    for &(tok, arrow) in OPS {
        if let Some(pos) = stmt.find(tok) {
            let better = match best {
                None => true,
                // Earlier position wins; at the same position the longer token
                // (found first in this longest-first list) already won.
                Some((bp, _, _)) => pos < bp,
            };
            if better {
                best = Some((pos, pos + tok.len(), arrow));
            }
        }
    }
    best
}

/// Split `X as Alias` at the ` as ` separator (whitespace-delimited).
fn split_as(s: &str) -> Option<(&str, &str)> {
    s.find(" as ").map(|at| (&s[..at], &s[at + 4..]))
}

/// If `stmt` begins with keyword `kw` followed by whitespace or `:` (or is
/// exactly `kw`), return the remainder (trimmed of a leading separator).
fn keyword<'a>(stmt: &'a str, kw: &str) -> Option<&'a str> {
    let rest = stmt.strip_prefix(kw)?;
    if rest.is_empty() {
        return Some(rest);
    }
    let c = rest.chars().next().unwrap();
    if c.is_whitespace() || c == ':' {
        Some(rest.trim_start())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::SeqEvent;

    fn messages(d: &SequenceDiagram) -> Vec<&SeqMessage> {
        d.events
            .iter()
            .filter_map(|e| match e {
                SeqEvent::Message(m) => Some(m),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn basic_participants_and_messages() {
        let d = parse("sequenceDiagram\n    Alice->>John: Hello John\n    John-->>Alice: Great!\n").unwrap();
        assert_eq!(d.participants.len(), 2);
        assert_eq!(d.participants[0].id, "Alice");
        assert_eq!(d.participants[1].id, "John");
        let m = messages(&d);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].arrow, SeqArrow::SolidArrow);
        assert_eq!(m[0].text, "Hello John");
        assert_eq!(m[1].arrow, SeqArrow::DottedArrow);
        assert_eq!(m[1].from, 1);
        assert_eq!(m[1].to, 0);
    }

    #[test]
    fn participant_alias_and_declaration_order() {
        let d = parse("sequenceDiagram\nparticipant J as John\nAlice->>J: Hi\n").unwrap();
        // J declared first, so it's index 0; Alice auto-created at index 1.
        assert_eq!(d.participants[0].id, "J");
        assert_eq!(d.participants[0].label, "John");
        assert_eq!(d.participants[1].id, "Alice");
    }

    #[test]
    fn arrow_variants() {
        let d = parse(
            "sequenceDiagram\nA->>B: a\nA-->>B: b\nA->B: c\nA-->B: d\nA-xB: e\nA--xB: f\nA-)B: g\nA--)B: h\n",
        )
        .unwrap();
        let got: Vec<SeqArrow> = messages(&d).iter().map(|m| m.arrow).collect();
        assert_eq!(
            got,
            vec![
                SeqArrow::SolidArrow,
                SeqArrow::DottedArrow,
                SeqArrow::SolidOpen,
                SeqArrow::DottedOpen,
                SeqArrow::SolidCross,
                SeqArrow::DottedCross,
                SeqArrow::SolidPoint,
                SeqArrow::DottedPoint,
            ]
        );
    }

    #[test]
    fn activation_suffix() {
        let d = parse("sequenceDiagram\nAlice->>+John: hi\nJohn-->>-Alice: bye\n").unwrap();
        let m = messages(&d);
        assert!(m[0].activate && !m[0].deactivate);
        assert!(m[1].deactivate && !m[1].activate);
    }

    #[test]
    fn note_and_title_and_autonumber() {
        let d = parse(
            "sequenceDiagram\ntitle: My Seq\nautonumber\nNote over Alice,Bob: hello\nAlice->>Bob: hi\n",
        )
        .unwrap();
        assert_eq!(d.title.as_deref(), Some("My Seq"));
        assert!(matches!(d.events[0], SeqEvent::Autonumber(Some((1, 1)))));
        let note = d.events.iter().find_map(|e| match e {
            SeqEvent::Note(n) => Some(n),
            _ => None,
        });
        assert!(note.is_some());
        assert_eq!(note.unwrap().actors.len(), 2);
    }

    #[test]
    fn blocks() {
        let d = parse("sequenceDiagram\nloop every day\nA->>B: x\nend\n").unwrap();
        assert!(matches!(&d.events[0], SeqEvent::Block(BlockBoundary::LoopStart(l)) if l == "every day"));
        assert!(matches!(d.events.last(), Some(SeqEvent::Block(BlockBoundary::LoopEnd))));
    }
}
