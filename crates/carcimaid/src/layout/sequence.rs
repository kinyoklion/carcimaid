//! Geometry for `sequenceDiagram`, ported from mermaid's `sequenceRenderer`.
//!
//! Milestone: box participants + messages + autonumber + title. Notes, blocks
//! (loop/alt/opt/par/rect) and activations parse but do not yet contribute
//! geometry — they will be layered on next. See `scratchpad/seq_spec.md`.

use crate::ir::{SeqArrow, SeqEvent, SequenceDiagram};
use crate::text;

// --- mermaid sequence config defaults (see SequenceDiagramConfig). ---
const DIAGRAM_MARGIN_X: f64 = 50.0;
const DIAGRAM_MARGIN_Y: f64 = 10.0;
const ACTOR_MARGIN: f64 = 50.0;
const ACTOR_WIDTH: f64 = 150.0;
const ACTOR_HEIGHT: f64 = 65.0;
const BOX_MARGIN: f64 = 10.0;
const WRAP_PADDING: f64 = 10.0;
const BOTTOM_MARGIN_ADJ: f64 = 1.0;
/// Actor/message label font size. mermaid's CLI emits actor and message labels
/// at 16px (the schema's 14 is overridden at runtime); both are calibrated to
/// the oracle here.
const LABEL_FONT: f64 = 16.0;
/// Per-line message text height (mermaid's rounded bbox height at 16px). The
/// vertical message stepping is very sensitive to this; calibrated to the
/// oracle (single-line message line sits at `cursor + 2*19 + boxMargin`).
const SEQ_LINE_HEIGHT: f64 = 19.0;

/// A laid-out sequence diagram: absolute geometry ready to render.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutSequence {
    /// SVG `width`/`height` attributes (mermaid's pre-title box + margins).
    pub width: f64,
    pub height: f64,
    /// viewBox `min-x min-y w h`.
    pub vb_min_x: f64,
    pub vb_min_y: f64,
    pub vb_w: f64,
    pub vb_h: f64,
    pub actors: Vec<PlacedActor>,
    pub messages: Vec<PlacedMessage>,
    /// Y of the top actor boxes (0) and the bottom (mirror) boxes.
    pub top_y: f64,
    pub bottom_y: f64,
    pub actor_height: f64,
    pub title: Option<String>,
}

/// A participant with its lifeline x and box width.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedActor {
    pub id: String,
    pub label: String,
    pub is_actor: bool,
    /// Left edge of the actor box.
    pub x: f64,
    pub width: f64,
}

impl PlacedActor {
    /// Lifeline / box centre x.
    pub fn cx(&self) -> f64 {
        self.x + self.width / 2.0
    }
}

/// A message arrow with its computed endpoints and label position.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedMessage {
    pub from: usize,
    pub to: usize,
    pub text: String,
    pub arrow: SeqArrow,
    /// Arrow line y (both endpoints share it — messages are horizontal).
    pub line_y: f64,
    /// Label baseline y.
    pub text_y: f64,
    pub start_x: f64,
    pub stop_x: f64,
    /// Autonumber index, if numbering is active for this message.
    pub seq_num: Option<i64>,
}

/// The widest line width of a `<br>`-separated label at `font`.
fn label_width(text: &str, font: f64) -> f64 {
    split_lines(text)
        .iter()
        .map(|l| text::measure_width(l, font))
        .fold(0.0_f64, f64::max)
}

/// Split a label on `<br>`/`<br/>` and newlines into display lines.
fn split_lines(text: &str) -> Vec<String> {
    text.replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .split('\n')
        .map(|s| s.trim().to_string())
        .collect()
}

/// `true` if this arrow paints a marker at its target (so the line is pulled
/// back from the actor centre to seat the marker). Open arrows (`->`, `-->`)
/// draw no marker and reach the centre.
fn has_end_marker(arrow: SeqArrow) -> bool {
    !matches!(arrow, SeqArrow::SolidOpen | SeqArrow::DottedOpen)
}

/// Lay out a sequence diagram.
pub fn layout(d: &SequenceDiagram) -> LaidOutSequence {
    // 1. Actor widths from label measurement (≥ conf.width).
    let widths: Vec<f64> = d
        .participants
        .iter()
        .map(|p| ACTOR_WIDTH.max(label_width(&p.label, LABEL_FONT) + 2.0 * WRAP_PADDING))
        .collect();
    let n = d.participants.len();

    // 2. Widest message label between each adjacent actor pair (left-actor key).
    //    Only adjacent pairs widen spacing (mermaid's getMaxMessageWidthPerActor).
    let mut max_msg_w = vec![0.0_f64; n.max(1)];
    for ev in &d.events {
        if let SeqEvent::Message(m) = ev {
            let mw = text::measure_width(&split_lines(&m.text).join(" "), LABEL_FONT)
                + 2.0 * WRAP_PADDING;
            let (lo, hi) = (m.from.min(m.to), m.from.max(m.to));
            if m.from == m.to {
                max_msg_w[lo] = max_msg_w[lo].max(mw / 2.0);
            } else if hi == lo + 1 {
                max_msg_w[lo] = max_msg_w[lo].max(mw);
            }
        }
    }

    // 3. Per-actor margin to its next actor.
    let mut margins = vec![ACTOR_MARGIN; n.max(1)];
    for i in 0..n {
        if max_msg_w[i] <= 0.0 {
            continue;
        }
        let next_half = if i + 1 < n { widths[i + 1] / 2.0 } else { 0.0 };
        let w = max_msg_w[i] + ACTOR_MARGIN - widths[i] / 2.0 - next_half;
        margins[i] = w.max(ACTOR_MARGIN);
    }

    // 4. Actor x positions.
    let mut xs = vec![0.0_f64; n];
    let (mut prev_width, mut prev_margin) = (0.0_f64, 0.0_f64);
    for i in 0..n {
        xs[i] = prev_width + prev_margin;
        prev_width += widths[i] + prev_margin;
        prev_margin = margins[i];
    }

    let actors: Vec<PlacedActor> = d
        .participants
        .iter()
        .enumerate()
        .map(|(i, p)| PlacedActor {
            id: p.id.clone(),
            label: p.label.clone(),
            is_actor: p.is_actor,
            x: xs[i],
            width: widths[i],
        })
        .collect();

    let max_actor_h = ACTOR_HEIGHT; // single-line labels for now
    let top_y = 0.0;

    // 5. Message vertical stepping.
    let mut cursor = max_actor_h; // bumpVerticalPos(maxHeight) after actors
    let mut messages = Vec::new();
    // autonumber state: Some((next_index, step)) when active.
    let mut autonum: Option<(i64, i64)> = None;
    for ev in &d.events {
        match ev {
            SeqEvent::Autonumber(v) => autonum = *v,
            SeqEvent::Message(m) => {
                let lines = split_lines(&m.text).len().max(1) as f64;
                let text_h = lines * SEQ_LINE_HEIGHT;
                let line_height = SEQ_LINE_HEIGHT;
                let c = cursor;
                let line_y = c + line_height + text_h + BOX_MARGIN;
                let text_y = (c + 15.0).round();

                let (fcx, tcx) = (actors[m.from].cx(), actors[m.to].cx());
                let (start_x, stop_x) = if m.from <= m.to {
                    let mut stop = tcx - 1.0;
                    if has_end_marker(m.arrow) {
                        stop -= 3.0;
                    }
                    (fcx + 1.0, stop)
                } else {
                    let mut stop = tcx + 1.0;
                    if has_end_marker(m.arrow) {
                        stop += 3.0;
                    }
                    (fcx - 1.0, stop)
                };

                let seq_num = autonum.map(|(idx, _)| idx);
                if let Some((idx, step)) = autonum {
                    autonum = Some((idx + step, step));
                }

                messages.push(PlacedMessage {
                    from: m.from,
                    to: m.to,
                    text: m.text.clone(),
                    arrow: m.arrow,
                    line_y,
                    text_y,
                    start_x,
                    stop_x,
                    seq_num,
                });
                cursor = line_y;
            }
            _ => {} // notes / blocks / activations: geometry TBD
        }
    }

    // 6. Bottom (mirror) actors.
    cursor += BOX_MARGIN * 2.0;
    let bottom_y = cursor;
    cursor += max_actor_h + BOX_MARGIN;

    // 7. Content bounding box.
    let box_startx = actors.iter().map(|a| a.x).fold(0.0_f64, f64::min);
    let box_stopx = actors
        .iter()
        .map(|a| a.x + a.width)
        .fold(0.0_f64, f64::max);
    let box_starty = top_y;
    let box_stopy = cursor;

    // 8. Overall size & viewBox (mirrorActors is always on for us).
    let box_h = box_stopy - box_starty;
    let height = box_h + 2.0 * DIAGRAM_MARGIN_Y - BOX_MARGIN + BOTTOM_MARGIN_ADJ;
    let box_w = box_stopx - box_startx;
    let width = box_w + 2.0 * DIAGRAM_MARGIN_X;
    let extra_title = if d.title.is_some() { 40.0 } else { 0.0 };

    LaidOutSequence {
        width,
        height,
        vb_min_x: box_startx - DIAGRAM_MARGIN_X,
        vb_min_y: -(DIAGRAM_MARGIN_Y + extra_title),
        vb_w: width,
        vb_h: height + extra_title,
        actors,
        messages,
        top_y,
        bottom_y,
        actor_height: max_actor_h,
        title: d.title.clone(),
    }
}
