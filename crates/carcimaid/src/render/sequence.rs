//! SVG emission for `sequenceDiagram`, aligned to mermaid's sequence DOM.
//!
//! Document order (driven by mermaid's `.lower()` calls; see `seq_spec.md`):
//! ```text
//! [bottom actor <g> …reverse] [top actor <g> …reverse (lifeline + inner g)]
//! <style> <g/> <defs symbols> <defs markers> [message <text>/<line> …] [title]
//! ```

use super::seq_defs;
use super::ID;
use crate::ir::SeqArrow;
use crate::layout::sequence::{LaidOutSequence, PlacedMessage};
use std::fmt::Write;

/// Render a laid-out sequence diagram to an SVG document string.
pub fn to_svg(s: &LaidOutSequence) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        concat!(
            r#"<svg aria-roledescription="sequence" role="graphics-document document" "#,
            r#"style="background-color: white;" xmlns="http://www.w3.org/2000/svg" "#,
            r#"xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" "#,
            r#"viewBox="{vx} {vy} {vw} {vh}" id="{id}">"#,
        ),
        w = n(s.width),
        h = n(s.height),
        vx = n(s.vb_min_x),
        vy = n(s.vb_min_y),
        vw = n(s.vb_w),
        vh = n(s.vb_h),
        id = ID,
    );

    // 1. Bottom (footer) actor boxes, reverse participant order.
    for a in s.actors.iter().rev() {
        actor_box(&mut out, a.cx(), a.x, s.bottom_y, a.width, s.actor_height, &a.label, &a.id, false);
    }
    // 2. Top actor boxes with lifelines, reverse participant order (id carries
    //    the forward index, matching mermaid's `actor{N}`/`root-{N}`).
    for (i, a) in s.actors.iter().enumerate().rev() {
        let cx = a.cx();
        let _ = write!(out, "<g>");
        let _ = write!(
            out,
            concat!(
                r#"<line id="actor{i}" x1="{cx}" y1="{y1}" x2="{cx}" y2="{y2}" "#,
                r##"class="actor-line 200" stroke-width="0.5px" stroke="#999" "##,
                r#"name="{name}" data-et="life-line" data-id="{name}"/>"#,
            ),
            i = i,
            cx = n(cx),
            y1 = n(s.top_y + s.actor_height),
            y2 = n(s.bottom_y),
            name = esc(&a.id),
        );
        let _ = write!(
            out,
            r#"<g id="root-{i}" data-et="participant" data-type="participant" data-id="{id}">"#,
            i = i,
            id = esc(&a.id),
        );
        actor_box(&mut out, cx, a.x, s.top_y, a.width, s.actor_height, &a.label, &a.id, true);
        let _ = write!(out, "</g></g>");
    }

    // 3. <style> (content is not structurally compared; kept compact but real).
    out.push_str(&style_block());
    // 4. Empty <g> skeleton mermaid leaves behind.
    out.push_str("<g></g>");
    // 5. Actor icon symbols + 6. arrow markers (verbatim from the oracle).
    out.push_str(seq_defs::SYMBOL_DEFS);
    out.push_str(seq_defs::MARKER_DEFS);

    // 7. Notes (all notes precede all messages in mermaid's DOM), then
    //    messages: text then line (+ optional autonumber circle/number).
    for note in &s.notes {
        note_box(&mut out, note);
    }
    for m in &s.messages {
        message(&mut out, m, &s.actors);
    }

    // 8. Title.
    if let Some(t) = &s.title {
        let box_w = s.width - 100.0; // 2*diagramMarginX
        let _ = write!(
            out,
            r#"<text x="{x}" y="-25" class="text">{t}</text>"#,
            x = n(box_w / 2.0 - 100.0),
            t = esc(t),
        );
    }

    out.push_str("</svg>");
    out
}

/// Emit an actor box rect + centred label. `top` selects the top vs bottom
/// (`actor-top`/`actor-bottom`) class.
#[allow(clippy::too_many_arguments)]
fn actor_box(
    out: &mut String,
    cx: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    label: &str,
    id: &str,
    top: bool,
) {
    let class = if top { "actor actor-top" } else { "actor actor-bottom" };
    if !top {
        out.push_str("<g>");
    }
    let _ = write!(
        out,
        concat!(
            r##"<rect x="{x}" y="{y}" fill="#eaeaea" stroke="#666" width="{w}" height="{h}" "##,
            r#"name="{name}" rx="3" ry="3" class="{class}"/>"#,
        ),
        x = n(x),
        y = n(y),
        w = n(w),
        h = n(h),
        name = esc(id),
        class = class,
    );
    let _ = write!(
        out,
        concat!(
            r#"<text x="{cx}" y="{ty}" dominant-baseline="central" alignment-baseline="central" "#,
            r#"class="actor actor-box" style="text-anchor: middle; font-size: 16px; font-weight: 400;">"#,
        ),
        cx = n(cx),
        ty = n(y + h / 2.0),
    );
    let _ = write!(out, r#"<tspan x="{cx}" dy="0">{label}</tspan></text>"#, cx = n(cx), label = esc(label));
    if !top {
        out.push_str("</g>");
    }
}

/// Emit a note: `<g data-et="note">` wrapping a `note` rect + `noteText`.
fn note_box(out: &mut String, note: &crate::layout::sequence::PlacedNote) {
    let cx = note.x + note.width / 2.0;
    let _ = write!(
        out,
        r#"<g data-et="note" data-id="i{id}">"#,
        id = note.id,
    );
    let _ = write!(
        out,
        concat!(
            r##"<rect x="{x}" y="{y}" fill="#EDF2AE" stroke="#666" width="{w}" height="{h}" "##,
            r#"class="note"/>"#,
        ),
        x = n(note.x),
        y = n(note.y),
        w = n(note.width),
        h = n(note.height),
    );
    let _ = write!(
        out,
        concat!(
            r#"<text x="{cx}" y="{ty}" text-anchor="middle" dominant-baseline="middle" "#,
            r#"alignment-baseline="middle" class="noteText" dy="1em" "#,
            r#"style="font-size: 16px; font-weight: 400;"><tspan x="{cx}">{t}</tspan></text></g>"#,
        ),
        cx = n(cx),
        ty = n(note.y + 5.0),
        t = esc(&note.text),
    );
}

/// Emit one message: label `<text>`, arrow `<line>`, and autonumber elements.
fn message(out: &mut String, m: &PlacedMessage, actors: &[crate::layout::sequence::PlacedActor]) {
    let (lo, hi) = (m.start_x.min(m.stop_x), m.start_x.max(m.stop_x));
    let text_x = (lo + (hi - lo) / 2.0).round();
    let _ = write!(
        out,
        concat!(
            r#"<text x="{x}" y="{y}" text-anchor="middle" dominant-baseline="middle" "#,
            r#"alignment-baseline="middle" style="font-size: 16px; font-weight: 400;" "#,
            r#"class="messageText" dy="1em">{t}</text>"#,
        ),
        x = n(text_x),
        y = n(m.text_y),
        t = esc(&m.text),
    );

    let dotted = m.arrow.is_dotted();
    let class = if dotted { "messageLine1" } else { "messageLine0" };
    let style = if dotted {
        r#" style="stroke-dasharray: 3, 3; fill: none;""#
    } else {
        r#" style="fill: none;""#
    };
    let end_marker = end_marker_id(m.arrow);
    let marker_end = match end_marker {
        Some(mk) => format!(r#" marker-end="url(#{ID}-{mk})""#),
        None => String::new(),
    };
    let marker_start = if matches!(m.arrow, SeqArrow::BiSolid | SeqArrow::BiDotted) {
        format!(r#" marker-start="url(#{ID}-arrowhead)""#)
    } else {
        String::new()
    };
    let _ = write!(
        out,
        concat!(
            r#"<line x1="{x1}" y1="{y}" x2="{x2}" y2="{y}" class="{class}" "#,
            r#"data-et="message" data-id="i{idx}" data-from="{from}" data-to="{to}" "#,
            r#"stroke-width="2" stroke="none"{me}{ms}{style}/>"#,
        ),
        x1 = n(m.start_x),
        x2 = n(m.stop_x),
        y = n(m.line_y),
        class = class,
        idx = m.id,
        from = esc(&actors[m.from].id),
        to = esc(&actors[m.to].id),
        me = marker_end,
        ms = marker_start,
        style = style,
    );

    if let Some(seq) = m.seq_num {
        // Autonumber circle (drawn by the zero-length line's marker) + number.
        let ax = if m.from <= m.to { m.start_x } else { m.start_x };
        let fs = if seq >= 100000 {
            "7px"
        } else if seq >= 1000 {
            "9px"
        } else {
            "12px"
        };
        let _ = write!(
            out,
            concat!(
                r#"<line x1="{ax}" y1="{y}" x2="{ax}" y2="{y}" stroke-width="0" "#,
                r#"marker-start="url(#{id}-sequencenumber)"/>"#,
                r#"<text x="{ax}" y="{ty}" font-family="sans-serif" font-size="{fs}" "#,
                r#"text-anchor="middle" class="sequenceNumber">{seq}</text>"#,
            ),
            ax = n(ax),
            y = n(m.line_y),
            id = ID,
            ty = n(m.line_y + 4.0),
            fs = fs,
            seq = seq,
        );
    }
}

/// The marker id (suffix) painted at a message's target, or `None` for open
/// arrows which draw no head.
fn end_marker_id(arrow: SeqArrow) -> Option<&'static str> {
    match arrow {
        SeqArrow::SolidArrow | SeqArrow::DottedArrow | SeqArrow::BiSolid | SeqArrow::BiDotted => {
            Some("arrowhead")
        }
        SeqArrow::SolidCross | SeqArrow::DottedCross => Some("crosshead"),
        SeqArrow::SolidPoint | SeqArrow::DottedPoint => Some("filled-head"),
        SeqArrow::SolidOpen | SeqArrow::DottedOpen => None,
    }
}

/// A compact sequence `<style>` block. Structural comparison ignores `<style>`
/// text, so this only needs to carry the visual essentials (line/text colours).
fn style_block() -> String {
    format!(
        concat!(
            "<style>#{id}{{font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:16px;fill:#333;}}",
            "#{id} .actor{{stroke:#9370DB;fill:#ECECFF;}}",
            "#{id} text.actor>tspan{{fill:black;stroke:none;}}",
            "#{id} .actor-line{{stroke:#999;}}",
            "#{id} .messageLine0{{stroke-width:1.5;stroke-dasharray:none;stroke:#333;}}",
            "#{id} .messageLine1{{stroke-width:1.5;stroke-dasharray:2,2;stroke:#333;}}",
            "#{id} .messageText{{fill:#333;stroke:none;font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:16px;}}",
            "#{id} .sequenceNumber{{fill:white;}}",
            "#{id} #arrowhead path{{fill:#333;stroke:#333;}}",
            "#{id} .note{{stroke:#aaaa33;fill:#fff5ad;}}",
            "#{id} .noteText{{fill:black;stroke:none;font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:14px;}}",
            "</style>",
        ),
        id = ID,
    )
}

/// Format a coordinate: integers without a decimal point, else trimmed to 3dp
/// (matching mermaid's compact numeric output).
fn n(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// Minimal XML text/attribute escaping.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
