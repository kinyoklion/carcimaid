//! Geometry for `sequenceDiagram`, ported from mermaid's `sequenceRenderer`.
//!
//! Milestone: box participants + messages + autonumber + title. Notes, blocks
//! (loop/alt/opt/par/rect) and activations parse but do not yet contribute
//! geometry — they will be layered on next. See `scratchpad/seq_spec.md`.

use crate::ir::{BlockBoundary, NotePlacement, SeqArrow, SeqEvent, SequenceDiagram};
use crate::text;

// --- mermaid sequence config defaults (see SequenceDiagramConfig). ---
const DIAGRAM_MARGIN_X: f64 = 50.0;
const DIAGRAM_MARGIN_Y: f64 = 10.0;
const ACTOR_MARGIN: f64 = 50.0;
const ACTOR_WIDTH: f64 = 150.0;
const ACTOR_HEIGHT: f64 = 65.0;
const BOX_MARGIN: f64 = 10.0;
const BOX_TEXT_MARGIN: f64 = 5.0;
const NOTE_MARGIN: f64 = 10.0;
const WRAP_PADDING: f64 = 10.0;
const BOTTOM_MARGIN_ADJ: f64 = 1.0;
const LABEL_BOX_HEIGHT: f64 = 20.0;
const ACTIVATION_WIDTH: f64 = 10.0;
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
    pub notes: Vec<PlacedNote>,
    /// Control-structure boxes (loop/alt/opt/par), in event order.
    pub blocks: Vec<PlacedBlock>,
    /// Activation bars.
    pub activations: Vec<PlacedActivation>,
    /// Coloured `rect` background regions (drawn behind everything).
    pub rects: Vec<PlacedRect>,
    /// Participant `box` groupings (drawn behind actors).
    pub boxes: Vec<PlacedBox>,
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
    /// Box height (`ACTOR_HEIGHT`, or taller for a wrapped multi-line label).
    pub height: f64,
    /// Label display lines (wrapped / `<br>`-split).
    pub label_lines: Vec<String>,
    /// Top-box y. `top_y` normally; for a `create`d actor, the y of the message
    /// that introduces it (box centred on that line).
    pub starty: f64,
    /// Lifeline end y. `bottom_y` normally; for a `destroy`ed actor, the y of
    /// the message that destroys it (marked with an X, no footer box).
    pub stopy: f64,
    /// `create`d mid-diagram (no top box at `top_y`).
    pub created: bool,
    /// `destroy`ed (lifeline ends at `stopy` with an X; no footer box).
    pub destroyed: bool,
}

impl PlacedActor {
    /// Lifeline / box centre x.
    pub fn cx(&self) -> f64 {
        self.x + self.width / 2.0
    }
}

/// An activation bar on a participant's lifeline.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedActivation {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// `class="activation{class_idx}"` (0/1/2, cycling by remaining stack depth).
    pub class_idx: usize,
    /// Stream id of the opening event (for document ordering).
    pub order: usize,
}

/// Tracks the activation-bar stack while walking the event stream.
#[derive(Default)]
struct ActivationState {
    stack: Vec<ActiveBar>,
    done: Vec<PlacedActivation>,
}

struct ActiveBar {
    actor: usize,
    startx: f64,
    stopx: f64,
    starty: f64,
    order: usize,
}

impl ActivationState {
    fn count(&self, actor: usize) -> usize {
        self.stack.iter().filter(|b| b.actor == actor).count()
    }
    /// Open a bar on `actor` at `starty`. Stacked bars step right by half the
    /// activation width (mermaid's `newActivation`).
    fn start(&mut self, actor: usize, actors: &[PlacedActor], starty: f64, order: usize) {
        let stacked = self.count(actor) as f64;
        let cx = actors[actor].cx();
        let x = cx + (stacked - 1.0) * ACTIVATION_WIDTH / 2.0;
        self.stack.push(ActiveBar { actor, startx: x, stopx: x + ACTIVATION_WIDTH, starty, order });
    }
    /// Close the most recent bar on `actor` at `cursor`, emitting the rect.
    fn end(&mut self, actor: usize, cursor: f64) {
        let Some(pos) = self.stack.iter().rposition(|b| b.actor == actor) else { return };
        let bar = self.stack.remove(pos);
        let remaining = self.count(actor);
        let (mut y, mut stopy) = (bar.starty, cursor);
        // Very short activations are nudged so the bar stays visible.
        if y + 18.0 > cursor {
            y = cursor - 6.0;
            stopy = cursor + 12.0;
        }
        self.done.push(PlacedActivation {
            x: bar.startx,
            y,
            w: ACTIVATION_WIDTH,
            h: stopy - y,
            class_idx: remaining % 3,
            order: bar.order,
        });
    }
    /// `[left, right]` bounds of `actor` — `cx ± 1` widened by any open bars.
    fn bounds(&self, actor: usize, actors: &[PlacedActor]) -> (f64, f64) {
        let cx = actors[actor].cx();
        let (mut l, mut r) = (cx - 1.0, cx + 1.0);
        for b in self.stack.iter().filter(|b| b.actor == actor) {
            l = l.min(b.startx);
            r = r.max(b.stopx);
        }
        (l, r)
    }
    fn finish(self) -> Vec<PlacedActivation> {
        self.done
    }
}

/// A control-structure box (loop / alt / opt / par) with its label, condition,
/// and section dividers.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedBlock {
    /// Event-stream index of the closing `end` (mermaid's `data-id`).
    pub id: usize,
    /// Keyword label shown in the corner tab (`loop`/`alt`/`opt`/`par`).
    pub label: String,
    /// The condition/title text (`[…]`), if any.
    pub title: String,
    pub startx: f64,
    pub stopx: f64,
    pub starty: f64,
    pub stopy: f64,
    /// `else`/`and` section dividers: `(y, title)`.
    pub sections: Vec<(f64, String)>,
}

/// A note box attached to one or more participants.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedNote {
    /// Event-stream index (mermaid's `data-id="i{id}"`).
    pub id: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Display lines (already wrapped / `<br>`-split / entity-decoded).
    pub lines: Vec<String>,
}

/// A message arrow with its computed endpoints and label position.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedMessage {
    /// Event-stream index (mermaid's `data-id="i{id}"`).
    pub id: usize,
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
    /// A self-message (`from == to`), drawn as a cubic loop path.
    pub self_loop: bool,
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
        .map(|s| crate::text::decode_entities(s.trim()))
        .collect()
}

fn box_startx_actors(actors: &[PlacedActor]) -> f64 {
    actors.iter().map(|a| a.x).fold(0.0_f64, f64::min)
}
fn box_stopx_actors(actors: &[PlacedActor]) -> f64 {
    actors.iter().map(|a| a.x + a.width).fold(0.0_f64, f64::max)
}

/// Advance the cursor and stack/close a block for a [`BlockBoundary`] event.
/// `fallback_lo/hi` bound an empty block's box to the actor span.
#[allow(clippy::too_many_arguments)]
fn handle_block(
    b: &BlockBoundary,
    eid: usize,
    cursor: &mut f64,
    open: &mut Vec<OpenBlock>,
    blocks: &mut Vec<PlacedBlock>,
    rects: &mut Vec<PlacedRect>,
    fallback_lo: f64,
    fallback_hi: f64,
) {
    // The label extra height mermaid adds for a non-empty condition/section.
    let label_extra = |title: &str| {
        if title.trim().is_empty() {
            0.0
        } else {
            SEQ_LINE_HEIGHT.max(LABEL_BOX_HEIGHT)
        }
    };
    let start = |label: &str, title: &str, cursor: &mut f64, open: &mut Vec<OpenBlock>| {
        *cursor += BOX_MARGIN; // preMargin
        let starty = *cursor;
        *cursor += BOX_MARGIN + BOX_TEXT_MARGIN + label_extra(title); // postMargin + label
        open.push(OpenBlock {
            label: label.to_string(),
            title: title.to_string(),
            starty,
            minx: f64::INFINITY,
            maxx: f64::NEG_INFINITY,
            maxy: starty,
            sections: Vec::new(),
            is_rect: false,
            fill: String::new(),
        });
    };
    let section = |title: &str, cursor: &mut f64, open: &mut [OpenBlock]| {
        *cursor += BOX_MARGIN + BOX_TEXT_MARGIN;
        let y = *cursor;
        *cursor += BOX_MARGIN + label_extra(title);
        if let Some(b) = open.last_mut() {
            b.sections.push((y, title.to_string()));
        }
    };
    match b {
        BlockBoundary::LoopStart(t) => start("loop", t, cursor, open),
        BlockBoundary::AltStart(t) => start("alt", t, cursor, open),
        BlockBoundary::OptStart(t) => start("opt", t, cursor, open),
        BlockBoundary::ParStart(t) => start("par", t, cursor, open),
        BlockBoundary::AltElse(t) | BlockBoundary::ParAnd(t) => section(t, cursor, open),
        BlockBoundary::RectStart(color) => {
            // Coloured background region (rendered as a filled rect at the back).
            *cursor += BOX_MARGIN;
            open.push(OpenBlock {
                label: String::new(),
                title: String::new(),
                starty: *cursor,
                minx: f64::INFINITY,
                maxx: f64::NEG_INFINITY,
                maxy: *cursor,
                sections: Vec::new(),
                is_rect: true,
                fill: color.clone(),
            });
            *cursor += BOX_MARGIN;
        }
        // `end` (parsed as LoopEnd for every construct) / explicit ends: close
        // the top open block.
        BlockBoundary::LoopEnd
        | BlockBoundary::AltEnd
        | BlockBoundary::OptEnd
        | BlockBoundary::ParEnd
        | BlockBoundary::RectEnd => {
            let Some(b) = open.pop() else { return };
            let (lo, hi) = if b.minx.is_finite() {
                (b.minx, b.maxx)
            } else {
                (fallback_lo, fallback_hi)
            };
            let startx = lo - BOX_MARGIN;
            let stopx = hi + BOX_MARGIN;
            let stopy = b.maxy + BOX_MARGIN;
            *cursor = stopy;
            if b.is_rect {
                rects.push(PlacedRect {
                    x: startx,
                    y: b.starty,
                    w: stopx - startx,
                    h: stopy - b.starty,
                    fill: b.fill,
                });
            } else {
                blocks.push(PlacedBlock {
                    id: eid,
                    label: b.label,
                    title: b.title,
                    startx,
                    stopx,
                    starty: b.starty,
                    stopy,
                    sections: b.sections,
                });
            }
        }
    }
}

/// A block being accumulated between its start and `end` while walking events.
struct OpenBlock {
    label: String,
    title: String,
    starty: f64,
    /// Content bounding box (min/max x, max stop-y) of enclosed messages/notes.
    minx: f64,
    maxx: f64,
    maxy: f64,
    sections: Vec<(f64, String)>,
    is_rect: bool,
    /// Fill colour for `rect` background regions.
    fill: String,
}

/// A participant `box` grouping rect + label.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedBox {
    /// Rect geometry (already padded).
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Fill colour (CSS) or `None` (transparent).
    pub color: Option<String>,
    pub name: String,
    /// Centre x and baseline y of the label.
    pub label_cx: f64,
    pub label_y: f64,
}

/// A coloured `rect` background region.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub fill: String,
}

/// Expand every open block's content bbox with an enclosed element spanning
/// `[x0, x1]` horizontally down to `stopy`.
fn expand_open(open: &mut [OpenBlock], x0: f64, x1: f64, stopy: f64) {
    for b in open.iter_mut() {
        b.minx = b.minx.min(x0.min(x1));
        b.maxx = b.maxx.max(x0.max(x1));
        b.maxy = b.maxy.max(stopy);
    }
}

/// Note box `(startx, width)` from mermaid's `buildNoteModel`, by placement.
fn note_geometry(note: &crate::ir::SeqNote, actors: &[PlacedActor]) -> (f64, f64) {
    let text_w = label_width(&note.text, LABEL_FONT);
    let a = &actors[note.actors[0]];
    match note.placement {
        NotePlacement::RightOf => {
            let width = if note.wrap { ACTOR_WIDTH } else { a.width.max(text_w + 2.0 * NOTE_MARGIN) };
            (a.x + (a.width + ACTOR_MARGIN) / 2.0, width)
        }
        NotePlacement::LeftOf => {
            let width = if note.wrap { ACTOR_WIDTH } else { a.width.max(text_w + 2.0 * NOTE_MARGIN) };
            (a.x - width + (a.width - ACTOR_MARGIN) / 2.0, width)
        }
        NotePlacement::Over if note.actors.len() >= 2 => {
            let b = &actors[note.actors[1]];
            let width = ((a.x + a.width / 2.0) - (b.x + b.width / 2.0)).abs() + ACTOR_MARGIN;
            let startx = if a.x < b.x {
                a.x + a.width / 2.0 - ACTOR_MARGIN / 2.0
            } else {
                b.x + b.width / 2.0 - ACTOR_MARGIN / 2.0
            };
            (startx, width)
        }
        NotePlacement::Over => {
            // A wrapped note uses the fixed width (actor/conf.width) and wraps
            // the text to it; an unwrapped one grows to the (one-line) text.
            let width = if note.wrap {
                a.width.max(ACTOR_WIDTH)
            } else {
                a.width.max(ACTOR_WIDTH).max(text_w + 2.0 * NOTE_MARGIN)
            };
            (a.x + (a.width - width) / 2.0, width)
        }
    }
}

/// The display lines of a note: wrapped to `width` when `note.wrap`, else split
/// on `<br>`. Returns lines (entity-decoded).
fn note_lines(note: &crate::ir::SeqNote, width: f64) -> Vec<String> {
    if note.wrap {
        crate::text::wrap_label(&note.text, width - 2.0 * WRAP_PADDING, LABEL_FONT)
            .iter()
            .map(|words| crate::text::decode_entities(&words.join(" ")))
            .collect()
    } else {
        split_lines(&note.text)
    }
}

/// `true` if this arrow paints a marker at its target (so the line is pulled
/// back from the actor centre to seat the marker). Open arrows (`->`, `-->`)
/// draw no marker and reach the centre.
fn has_end_marker(arrow: SeqArrow) -> bool {
    // Only solid/filled heads pull the stop endpoint back by 3. Open arrows,
    // stick (open-line) directional heads, and reverse arrows (head at source)
    // reach the actor centre.
    !matches!(
        arrow,
        SeqArrow::SolidOpen
            | SeqArrow::DottedOpen
            | SeqArrow::StickTop
            | SeqArrow::StickBottom
            | SeqArrow::StickTopDotted
            | SeqArrow::StickBottomDotted
    ) && !arrow.is_reverse()
}

/// Lay out a sequence diagram.
pub fn layout(d: &SequenceDiagram) -> LaidOutSequence {
    let n = d.participants.len();
    // 1. Actor label lines, widths, and heights. A wrapped label uses the fixed
    //    width and grows the box height; an unwrapped one widens to its widest
    //    `<br>` line and stays one row tall.
    let actor_lines: Vec<Vec<String>> = d
        .participants
        .iter()
        .map(|p| {
            if p.wrap {
                crate::text::wrap_label(&p.label, ACTOR_WIDTH - 2.0 * WRAP_PADDING, LABEL_FONT)
                    .iter()
                    .map(|w| crate::text::decode_entities(&w.join(" ")))
                    .collect()
            } else {
                split_lines(&p.label)
            }
        })
        .collect();
    let widths: Vec<f64> = d
        .participants
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if p.wrap {
                ACTOR_WIDTH
            } else {
                let w = actor_lines[i]
                    .iter()
                    .map(|l| label_width(l, LABEL_FONT))
                    .fold(0.0_f64, f64::max);
                ACTOR_WIDTH.max(w + 2.0 * WRAP_PADDING)
            }
        })
        .collect();
    let heights: Vec<f64> = d
        .participants
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if p.wrap {
                (actor_lines[i].len() as f64 * SEQ_LINE_HEIGHT).max(ACTOR_HEIGHT)
            } else {
                ACTOR_HEIGHT
            }
        })
        .collect();

    // 2. Widest message label between each adjacent actor pair (left-actor key).
    //    Only adjacent pairs widen spacing (mermaid's getMaxMessageWidthPerActor).
    let mut max_msg_w = vec![0.0_f64; n.max(1)];
    for ev in &d.events {
        if let SeqEvent::Message(m) = ev {
            // A wrapped message contributes only conf.width; an unwrapped one
            // contributes its *widest* `<br>` line (not the joined text).
            let raw = if m.wrap {
                ACTOR_WIDTH
            } else {
                label_width(&m.text, LABEL_FONT)
            };
            let mw = raw + 2.0 * WRAP_PADDING;
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

    // 3b. Per-box margin (boxTextMargin, widened if the box label is wider than
    //     its members) + which participants belong to each box.
    let nb = d.boxes.len();
    let box_margin = vec![BOX_TEXT_MARGIN; nb]; // label-overflow term omitted (rare)

    // 4. Actor x positions, injecting box entry/exit margins (mermaid's
    //    addActorRenderingData). `box_x`/`box_right` bound each box.
    let mut xs = vec![0.0_f64; n];
    let mut box_x = vec![0.0_f64; nb];
    let mut box_right = vec![0.0_f64; nb];
    let mut box_seen = vec![false; nb];
    // Created participants get +width/2 extra left margin (room for the
    // introducing arrow); pre-scan the events for `create`.
    let mut is_created = vec![false; n];
    for ev in &d.events {
        if let SeqEvent::Create(a) = ev {
            is_created[*a] = true;
        }
    }
    let (mut prev_width, mut prev_margin) = (0.0_f64, 0.0_f64);
    let mut prev_box: Option<usize> = None;
    for i in 0..n {
        let bx = d.participants[i].box_idx;
        // Leaving a box.
        if let Some(pb) = prev_box {
            if Some(pb) != bx {
                prev_margin += BOX_MARGIN + box_margin[pb];
            }
        }
        // Entering a box.
        if let Some(b) = bx {
            if Some(b) != prev_box && !box_seen[b] {
                box_x[b] = prev_width + prev_margin;
                box_seen[b] = true;
                prev_margin += box_margin[b];
            }
        }
        if is_created[i] {
            prev_margin += widths[i] / 2.0;
        }
        xs[i] = prev_width + prev_margin;
        prev_width += widths[i] + prev_margin;
        if let Some(b) = bx {
            box_right[b] = prev_width + box_margin[b];
        }
        prev_margin = margins[i];
        prev_box = bx;
    }

    let mut actors: Vec<PlacedActor> = d
        .participants
        .iter()
        .enumerate()
        .map(|(i, p)| PlacedActor {
            id: p.id.clone(),
            label: p.label.clone(),
            is_actor: p.is_actor,
            x: xs[i],
            width: widths[i],
            height: heights[i],
            label_lines: actor_lines[i].clone(),
            starty: 0.0, // set to top_y below
            stopy: 0.0,  // set to bottom_y after the walk
            created: false,
            destroyed: false,
        })
        .collect();

    let max_actor_h = heights.iter().cloned().fold(ACTOR_HEIGHT, f64::max);
    // A participant `box` reserves space above the actors for its label, so the
    // whole diagram shifts down by boxMargin + label height when boxes exist.
    let has_boxes = box_seen.iter().any(|&s| s);
    let box_label_h = SEQ_LINE_HEIGHT; // single-line box names
    let top_y = if has_boxes { BOX_MARGIN + box_label_h } else { 0.0 };
    for a in &mut actors {
        a.starty = top_y;
    }

    // 5. Walk the event stream in order, stepping the shared vertical cursor.
    //    `sid` is the event-stream index used for `data-id`. mermaid inserts a
    //    phantom activeStart/End message for a `+`/`-` suffix, so `sid` skips an
    //    extra slot for those (keeping later ids aligned).
    let mut cursor = top_y + max_actor_h; // bumpVerticalPos(maxHeight) after actors
    let mut messages = Vec::new();
    let mut notes = Vec::new();
    let mut blocks: Vec<PlacedBlock> = Vec::new();
    let mut rects: Vec<PlacedRect> = Vec::new();
    let mut open: Vec<OpenBlock> = Vec::new();
    let mut acts = ActivationState::default();
    let mut autonum: Option<(i64, i64)> = None;
    // Participants awaiting their introducing/destroying message.
    let mut pending_create: Vec<usize> = Vec::new();
    let mut pending_destroy: Vec<usize> = Vec::new();
    let mut sid = 0usize;
    for ev in &d.events {
        let eid = sid;
        // `create`/`destroy` don't push a message, so they don't consume a
        // stream id (unlike autonumber/activate, which mermaid records).
        if !matches!(ev, SeqEvent::Create(_) | SeqEvent::Destroy(_)) {
            sid += 1;
        }
        match ev {
            SeqEvent::Autonumber(v) => autonum = *v,
            SeqEvent::Create(a) => {
                actors[*a].created = true;
                pending_create.push(*a);
            }
            SeqEvent::Destroy(a) => pending_destroy.push(*a),
            SeqEvent::Activate(a) => acts.start(*a, &actors, cursor, eid),
            SeqEvent::Deactivate(a) => acts.end(*a, cursor),
            SeqEvent::Block(b) => handle_block(
                b,
                eid,
                &mut cursor,
                &mut open,
                &mut blocks,
                &mut rects,
                box_startx_actors(&actors),
                box_stopx_actors(&actors),
            ),
            SeqEvent::Note(note) => {
                cursor += BOX_MARGIN; // bumpVerticalPos(boxMargin)
                let starty = cursor;
                let (nx, nw) = note_geometry(note, &actors);
                let lines = note_lines(note, nw);
                let n_lines = lines.len().max(1) as f64;
                let height = n_lines * SEQ_LINE_HEIGHT + 2.0 * NOTE_MARGIN;
                cursor += height;
                expand_open(&mut open, nx, nx + nw, starty + height);
                notes.push(PlacedNote {
                    id: eid,
                    x: nx,
                    y: starty,
                    width: nw,
                    height,
                    lines,
                });
            }
            SeqEvent::Message(m) => {
                // A wrapped message wraps to the space between its actors (the
                // spacing already reserved conf.width); the display text carries
                // the resulting `\n` breaks (render splits on them).
                let disp_text = if m.wrap {
                    let span = (actors[m.from].cx() - actors[m.to].cx()).abs();
                    let w = (span - 2.0 * WRAP_PADDING).max(ACTOR_WIDTH - 2.0 * WRAP_PADDING);
                    crate::text::wrap_label(&m.text, w, LABEL_FONT)
                        .iter()
                        .map(|words| crate::text::decode_entities(&words.join(" ")))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    m.text.clone()
                };
                let lines = split_lines(&disp_text).len().max(1) as f64;
                let text_h = lines * SEQ_LINE_HEIGHT;
                let line_height = SEQ_LINE_HEIGHT;
                let c = cursor;
                let line_y = c + line_height + text_h + BOX_MARGIN;
                let text_y = (c + 15.0).round();
                let self_loop = m.from == m.to;

                // A `+` suffix activates the target *before* the line is placed,
                // so the arrow already lands on the new bar's edge (phantom
                // activeStart consumes the next stream id).
                if m.activate {
                    acts.start(m.to, &actors, line_y, sid);
                    sid += 1;
                }

                // Endpoints from activation bounds (base = cx ± 1, matching the
                // no-activation case): near edge of source, far edge of target.
                let (fl, fr) = acts.bounds(m.from, &actors);
                let (tl, tr) = acts.bounds(m.to, &actors);
                let (mut start_x, mut stop_x, exp_lo, exp_hi);
                if self_loop {
                    // Self-message: a cubic loop bulging right to cx+61. The
                    // cursor advances 30 past the line (for the loop's height).
                    start_x = fr; // cx + 1 (or bar edge)
                    stop_x = fr;
                    exp_lo = fl;
                    exp_hi = fr + 61.0;
                    cursor = line_y + 30.0;
                } else {
                    let l2r = fl <= tl;
                    let mut sx = if l2r { fr } else { fl };
                    let mut ex = if l2r { tl } else { tr };
                    if has_end_marker(m.arrow) {
                        ex += if l2r { -3.0 } else { 3.0 };
                    }
                    // Bidirectional and *solid* reverse arrows have a filled head
                    // at the source (pull it back); stick reverse heads don't.
                    let solid_rev = matches!(
                        m.arrow,
                        SeqArrow::SolidTopRev
                            | SeqArrow::SolidBottomRev
                            | SeqArrow::SolidTopRevDotted
                            | SeqArrow::SolidBottomRevDotted
                    );
                    if matches!(m.arrow, SeqArrow::BiSolid | SeqArrow::BiDotted) || solid_rev {
                        sx += if l2r { 3.0 } else { -3.0 };
                    }
                    start_x = sx;
                    stop_x = ex;
                    exp_lo = fl.min(tl);
                    exp_hi = fr.max(tr);
                    cursor = line_y;
                }

                // create/destroy: the introducing/destroying message centres the
                // actor's box/endpoint on this line and bumps the cursor by half
                // an actor height. `adj` pulls the arrow to the box edge.
                let half_h = max_actor_h / 2.0;
                // create adds +3 to the pull-back; destroy does not.
                let half_w = |a: &PlacedActor| if a.is_actor { 18.0 } else { a.width / 2.0 };
                if let Some(pos) = pending_create.iter().position(|&x| x == m.to) {
                    pending_create.remove(pos);
                    actors[m.to].starty = line_y - half_h;
                    let a = half_w(&actors[m.to]) + 3.0;
                    stop_x += if actors[m.to].x < actors[m.from].x { a } else { -a };
                    cursor = line_y + half_h;
                } else if let Some(pos) = pending_destroy.iter().position(|&x| x == m.from) {
                    pending_destroy.remove(pos);
                    actors[m.from].stopy = line_y - half_h;
                    actors[m.from].destroyed = true;
                    let a = half_w(&actors[m.from]);
                    start_x += if actors[m.from].x < actors[m.to].x { a } else { -a };
                    cursor = line_y + half_h;
                } else if let Some(pos) = pending_destroy.iter().position(|&x| x == m.to) {
                    pending_destroy.remove(pos);
                    actors[m.to].stopy = line_y - half_h;
                    actors[m.to].destroyed = true;
                    let a = half_w(&actors[m.to]);
                    stop_x += if actors[m.to].x < actors[m.from].x { a } else { -a };
                    cursor = line_y + half_h;
                }

                // A `-` suffix ends the source's bar at this line.
                if m.deactivate {
                    acts.end(m.from, line_y);
                    sid += 1;
                }

                let seq_num = autonum.map(|(idx, _)| idx);
                if let Some((idx, step)) = autonum {
                    autonum = Some((idx + step, step));
                }

                // Loop bounds span the involved actors' outer edges (activation
                // bounds) / the self-loop bulge, not the adjusted endpoints.
                expand_open(&mut open, exp_lo, exp_hi, cursor);
                messages.push(PlacedMessage {
                    id: eid,
                    from: m.from,
                    to: m.to,
                    text: disp_text,
                    arrow: m.arrow,
                    line_y,
                    text_y,
                    start_x,
                    stop_x,
                    self_loop,
                    seq_num,
                });
            }
        }
    }
    let activations = acts.finish();

    // 6. Bottom (mirror) actors.
    cursor += BOX_MARGIN * 2.0;
    let bottom_y = cursor;
    cursor += max_actor_h + BOX_MARGIN;
    // Non-destroyed lifelines run to the bottom mirror boxes; destroyed ones
    // already have their (earlier) stopy.
    for a in &mut actors {
        if !a.destroyed {
            a.stopy = bottom_y;
        }
    }

    // 6c. Participant boxes span the full content height. Padding = boxMargin*2.
    let boxes: Vec<PlacedBox> = d
        .boxes
        .iter()
        .enumerate()
        .filter(|(b, _)| box_seen[*b])
        .map(|(b, sb)| {
            // The box rect starts at y=0 (above the shifted-down actors); its
            // label sits in the reserved band at top.
            let pad = BOX_MARGIN * 2.0;
            let bw = box_right[b] - box_x[b];
            let bh = cursor; // box.height = final verticalPos - box.y(0)
            PlacedBox {
                x: box_x[b] - pad,
                y: -pad * 0.25,
                w: bw + 2.0 * pad,
                h: bh + pad * 0.75,
                color: sb.color.clone(),
                name: sb.name.clone(),
                label_cx: box_x[b] + bw / 2.0,
                label_y: BOX_TEXT_MARGIN + box_label_h / 2.0,
            }
        })
        .collect();

    // 7. Content bounding box (actors + note extents; notes can overhang).
    let mut box_startx = actors.iter().map(|a| a.x).fold(0.0_f64, f64::min);
    let mut box_stopx = actors
        .iter()
        .map(|a| a.x + a.width)
        .fold(0.0_f64, f64::max);
    for note in &notes {
        box_startx = box_startx.min(note.x);
        box_stopx = box_stopx.max(note.x + note.width);
    }
    for b in &blocks {
        box_startx = box_startx.min(b.startx);
        box_stopx = box_stopx.max(b.stopx);
    }
    for a in &activations {
        box_startx = box_startx.min(a.x);
        box_stopx = box_stopx.max(a.x + a.w);
    }
    for r in &rects {
        box_startx = box_startx.min(r.x);
        box_stopx = box_stopx.max(r.x + r.w);
    }
    for m in &messages {
        // Self-loops bulge right to start_x + 61.
        let hi = if m.self_loop { m.start_x + 61.0 } else { m.start_x.max(m.stop_x) };
        box_startx = box_startx.min(m.start_x.min(m.stop_x));
        box_stopx = box_stopx.max(hi);
    }
    // Boxes contribute their *unpadded* extent to the diagram bounds (the
    // padded rect overflows visually but doesn't grow the viewBox); mermaid
    // inserts `box.x .. box.x + box.width`.
    for (b, seen) in box_seen.iter().enumerate() {
        if *seen {
            box_startx = box_startx.min(box_x[b]);
            box_stopx = box_stopx.max(box_right[b]);
        }
    }
    // A box grouping starts at y=0 (above the down-shifted actors).
    let box_starty = if has_boxes { 0.0 } else { top_y };
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
        notes,
        blocks,
        activations,
        rects,
        boxes,
        top_y,
        bottom_y,
        actor_height: max_actor_h,
        title: d.title.clone(),
    }
}
