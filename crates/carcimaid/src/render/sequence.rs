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

    // Lifeline start y differs for actor-type (glyph, +80) vs box (+65).
    // Lifeline runs from the bottom of the (possibly created-and-lowered) top
    // box to the actor's stopy (bottom, or the destroy point).
    let lifeline_y1 = |a: &crate::layout::sequence::PlacedActor| {
        a.starty + if a.is_actor { ACTOR_GLYPH_H } else { a.height }
    };

    // 0a. Participant `box` groupings (drawn behind everything, lowered to front).
    for b in &s.boxes {
        let _ = write!(out, "<g>");
        let fill = b.color.as_deref().unwrap_or("none");
        let _ = write!(
            out,
            concat!(
                r#"<rect x="{x}" y="{y}" fill="{f}" stroke="rgb(0,0,0, 0.5)" width="{w}" "#,
                r#"height="{h}" class="rect"/>"#,
            ),
            x = n(b.x),
            y = n(b.y),
            f = esc(fill),
            w = n(b.w),
            h = n(b.h),
        );
        if !b.name.trim().is_empty() {
            let _ = write!(
                out,
                concat!(
                    r#"<text x="{x}" y="{y}" dominant-baseline="central" alignment-baseline="central" "#,
                    r#"class="text" style="text-anchor: middle; font-size: 16px; font-weight: 400;">"#,
                    r#"<tspan x="{x}" dy="0">{t}</tspan></text>"#,
                ),
                x = n(b.label_cx),
                y = n(b.label_y),
                t = esc(&b.name),
            );
        }
        let _ = write!(out, "</g>");
    }

    // 0b. Coloured `rect` background regions (behind everything). Emitted in
    //     reverse close-order so an inner (later-listed) nested rect draws on
    //     top of its enclosing rect (mermaid lowers each, reversing the order).
    for r in s.rects.iter().rev() {
        let _ = write!(
            out,
            r#"<rect x="{x}" y="{y}" fill="{f}" width="{w}" height="{h}" class="rect"/>"#,
            x = n(r.x),
            y = n(r.y),
            f = esc(&r.fill),
            w = n(r.w),
            h = n(r.h),
        );
    }

    // 1. Bottom (footer) actors, reverse participant order. A box footer is a
    //    `<g>` with rect+label; an actor footer's *lowered* part is an empty
    //    `<g>` (its glyph is emitted later, after the defs).
    for a in s.actors.iter().rev() {
        // A destroyed actor's footer sits at its destroy point, not the bottom.
        let fy = if a.destroyed { a.stopy } else { s.bottom_y };
        if a.is_actor {
            out.push_str("<g></g>");
        } else if let Some(shape) = &a.shape {
            draw_shape(
                &mut out,
                shape,
                a.cx(),
                a.x,
                fy,
                a.width,
                a.height,
                &a.label_lines,
                &a.id,
                false,
            );
        } else {
            actor_box(
                &mut out,
                a.cx(),
                a.x,
                fy,
                a.width,
                a.height,
                &a.label_lines,
                &a.id,
                false,
            );
        }
    }
    // 2. Top actors with lifelines, reverse participant order (id carries the
    //    forward index, matching mermaid's `actor{N}`/`root-{N}`). For actors,
    //    only the lifeline `<g>` is lowered here; the glyph comes after defs.
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
            y1 = n(lifeline_y1(a)),
            y2 = n(a.stopy),
            name = esc(&a.id),
        );
        if !a.is_actor {
            let _ = write!(
                out,
                r#"<g id="root-{i}" data-et="participant" data-type="participant" data-id="{id}">"#,
                i = i,
                id = esc(&a.id),
            );
            if let Some(shape) = &a.shape {
                draw_shape(
                    &mut out,
                    shape,
                    cx,
                    a.x,
                    a.starty,
                    a.width,
                    a.height,
                    &a.label_lines,
                    &a.id,
                    true,
                );
            } else {
                actor_box(
                    &mut out,
                    cx,
                    a.x,
                    a.starty,
                    a.width,
                    a.height,
                    &a.label_lines,
                    &a.id,
                    true,
                );
            }
            out.push_str("</g>");
        }
        out.push_str("</g>");
    }

    // 3. <style> (content is not structurally compared; kept compact but real).
    out.push_str(&style_block());
    // 4. Empty <g> skeleton mermaid leaves behind.
    out.push_str("<g></g>");
    // 5. Actor icon symbols + 6. arrow markers (verbatim from the oracle).
    out.push_str(seq_defs::SYMBOL_DEFS);
    out.push_str(seq_defs::MARKER_DEFS);

    // 6b. Actor-type stick-figure glyphs (non-lowered, so they land here in
    //     participant order — all top glyphs, then all bottom glyphs). The
    //     torso/arms id index is the participant index for top glyphs, and the
    //     last actor index (n-1) for every bottom glyph (mermaid freezes its
    //     `actorCnt` during the footer pass).
    for (i, a) in s.actors.iter().enumerate().filter(|(_, a)| a.is_actor) {
        actor_glyph(&mut out, a.cx(), a.starty, &a.label_lines, &a.id, true, i);
    }

    // 7. Notes and control-structure boxes, in event order (both precede all
    //    messages in mermaid's DOM), then messages.
    let mut pre: Vec<(usize, PreElem)> = Vec::new();
    pre.extend(s.notes.iter().map(|nt| (nt.id, PreElem::Note(nt))));
    pre.extend(s.blocks.iter().map(|b| (b.id, PreElem::Block(b))));
    pre.extend(
        s.activations
            .iter()
            .map(|a| (a.order, PreElem::Activation(a))),
    );
    pre.sort_by_key(|(id, _)| *id);
    for (_, el) in &pre {
        match el {
            PreElem::Note(nt) => note_box(&mut out, nt),
            PreElem::Block(b) => block_box(&mut out, b),
            PreElem::Activation(a) => activation_bar(&mut out, a),
        }
    }
    for m in &s.messages {
        message(&mut out, m, &s.actors);
    }

    // Bottom (footer) actor glyphs are drawn after the messages (footer pass);
    // their torso/arms id index is frozen at the last actor index.
    let last = s.actors.len().saturating_sub(1);
    for a in s.actors.iter().filter(|a| a.is_actor) {
        let fy = if a.destroyed { a.stopy } else { s.bottom_y };
        actor_glyph(&mut out, a.cx(), fy, &a.label_lines, &a.id, false, last);
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

/// Height of the actor stick-figure glyph (its lifeline starts `actorY + 80`).
const ACTOR_GLYPH_H: f64 = 80.0;

/// Emit an `actor`-type stick-figure glyph (head/torso/arms/legs + label),
/// matching mermaid's `drawActorTypeActor`. `top` selects the top/bottom class.
fn actor_glyph(
    out: &mut String,
    cx: f64,
    y: f64,
    lines: &[String],
    id: &str,
    top: bool,
    n_idx: usize,
) {
    let class = if top {
        "actor-man actor-top"
    } else {
        "actor-man actor-bottom"
    };
    // The `data-*` participant attrs are only on the top glyph.
    let data = if top {
        r#" data-et="participant" data-type="actor""#
    } else {
        ""
    };
    let did = if top {
        format!(r#" data-id="{}""#, esc(id))
    } else {
        String::new()
    };
    let _ = write!(
        out,
        r#"<g class="{class}" name="{name}"{data}{did} style="stroke: rgb(147, 112, 219);">"#,
        class = class,
        name = esc(id),
        data = data,
        did = did,
    );
    // torso + arms carry ids; legs don't (offsets calibrated to the oracle).
    let _ = write!(
        out,
        r#"<line id="actor-man-torso{n}" x1="{cx}" y1="{y1}" x2="{cx}" y2="{y2}"/>"#,
        n = n_idx,
        cx = n(cx),
        y1 = n(y + 25.0),
        y2 = n(y + 45.0),
    );
    let _ = write!(
        out,
        r#"<line id="actor-man-arms{n}" x1="{x1}" y1="{yy}" x2="{x2}" y2="{yy}"/>"#,
        n = n_idx,
        x1 = n(cx - 18.0),
        x2 = n(cx + 18.0),
        yy = n(y + 33.0),
    );
    let l = |out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64| {
        let _ = write!(
            out,
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}"/>"#,
            n(x1),
            n(y1),
            n(x2),
            n(y2)
        );
    };
    l(out, cx - 18.0, y + 60.0, cx, y + 45.0); // left leg
    l(out, cx, y + 45.0, cx + 16.0, y + 60.0); // right leg
    let _ = write!(
        out,
        r#"<circle cx="{cx}" cy="{cy}" r="15" width="150" height="65"/>"#,
        cx = n(cx),
        cy = n(y + 10.0),
    );
    actor_label(out, cx, y + 67.5, lines, "actor actor-man");
    out.push_str("</g>");
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
    lines: &[String],
    id: &str,
    top: bool,
) {
    let class = if top {
        "actor actor-top"
    } else {
        "actor actor-bottom"
    };
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
    actor_label(out, cx, y + h / 2.0, lines, "actor actor-box");
    if !top {
        out.push_str("</g>");
    }
}

/// Emit a UML participant shape (`@{type}`): boundary/control/entity/database
/// (icon + label below) or queue/collections (box-like, label inside). Geometry
/// is a visual approximation of mermaid's shapes (which use browser `getBBox`).
#[allow(clippy::too_many_arguments)]
fn draw_shape(
    out: &mut String,
    shape: &str,
    cx: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    lines: &[String],
    id: &str,
    top: bool,
) {
    let cls = if top {
        "actor actor-top"
    } else {
        "actor actor-bottom"
    };
    let fill = r##"fill="#eaeaea" stroke="#666""##;
    if !top {
        out.push_str("<g>");
    }
    let circle = |out: &mut String, ccx: f64, ccy: f64, r: f64| {
        let _ = write!(
            out,
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" {f} name="{id}" class="{cls}"/>"#,
            cx = n(ccx),
            cy = n(ccy),
            r = n(r),
            f = fill,
            id = esc(id),
            cls = cls,
        );
    };
    let line = |out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64| {
        let _ = write!(
            out,
            r##"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="#666" stroke-width="1"/>"##,
            x1 = n(x1),
            y1 = n(y1),
            x2 = n(x2),
            y2 = n(y2),
        );
    };
    match shape {
        // Box-like shapes carry the label inside; the icon frames the whole box.
        "queue" => {
            // Horizontal cylinder (mermaid's queue): elliptical end caps.
            let ry = h / 2.0;
            let rx = ry / (2.5 + h / 50.0);
            let body = format!(
                "M {sx},{y} a {rx},{ry} 0 0 0 0,{h} h {hw} a {rx},{ry} 0 0 0 0,{nh} Z",
                sx = n(x + rx),
                y = n(y),
                rx = n(rx),
                ry = n(ry),
                h = n(h),
                hw = n(w - 2.0 * rx),
                nh = n(-h),
            );
            let cap = format!(
                "M {sx},{y} a {rx},{ry} 0 0 0 0,{h}",
                sx = n(x + w - rx),
                y = n(y),
                rx = n(rx),
                ry = n(ry),
                h = n(h),
            );
            let _ = write!(
                out,
                r#"<path d="{d}" {f} name="{id}" class="{cls}"/>"#,
                d = body,
                f = fill,
                id = esc(id),
                cls = cls
            );
            let _ = write!(
                out,
                r##"<path d="{d}" fill="none" stroke="#666"/>"##,
                d = cap
            );
            actor_label(out, cx, y + h / 2.0, lines, "actor actor-box");
        }
        "collections" => {
            // A shadow box offset behind, then the front box; label inside.
            let _ = write!(
                out,
                r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" {f} name="{id}" class="{cls}"/>"#,
                x = n(x + 6.0),
                y = n(y - 6.0),
                w = n(w),
                h = n(h),
                f = fill,
                id = esc(id),
                cls = cls,
            );
            let _ = write!(
                out,
                r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" {f} name="{id}" class="{cls}"/>"#,
                x = n(x),
                y = n(y),
                w = n(w),
                h = n(h),
                f = fill,
                id = esc(id),
                cls = cls,
            );
            actor_label(out, cx, y + h / 2.0, lines, "actor actor-box");
        }
        // Icon shapes: small glyph in the upper area, label centred below it.
        "database" => {
            let rw = 18.0;
            let ry = 5.0;
            let bh = 26.0;
            let (l, ty) = (cx - rw, y + 6.0);
            let d = format!(
                "M {l},{t} a {rw},{ry} 0 0 0 {d2},0 a {rw},{ry} 0 0 0 {nd2},0 l 0,{bh} a {rw},{ry} 0 0 0 {d2},0 l 0,{nbh}",
                l = n(l), t = n(ty + ry), rw = n(rw), ry = n(ry),
                d2 = n(rw * 2.0), nd2 = n(-rw * 2.0), bh = n(bh), nbh = n(-bh),
            );
            let _ = write!(
                out,
                r#"<path d="{d}" {f} name="{id}" class="{cls}"/>"#,
                d = d,
                f = fill,
                id = esc(id),
                cls = cls
            );
            actor_label(out, cx, y + h - 8.0, lines, "actor actor-box");
        }
        _ => {
            let (ccy, r) = (y + 18.0, 16.0);
            match shape {
                "boundary" => {
                    line(out, cx - r - 12.0, y + 4.0, cx - r - 12.0, y + 32.0);
                    line(out, cx - r - 12.0, ccy, cx - r, ccy);
                    circle(out, cx, ccy, r);
                }
                "entity" => {
                    circle(out, cx, ccy, r);
                    line(out, cx - r, ccy + r + 4.0, cx + r, ccy + r + 4.0);
                }
                // control: circle with mermaid's concave arrowhead flick at the
                // top (its `filled-head-control` marker, angled 172.5°).
                _ => {
                    circle(out, cx, ccy, r);
                    let _ = write!(
                        out,
                        concat!(
                            r##"<path d="M 14.4 5.6 L 7.2 10.4 L 8.8 5.6 L 7.2 0.8 Z" "##,
                            r##"transform="translate({tx},{ty}) rotate(172.5) translate(-11,-5.8)" "##,
                            r##"fill="#666" stroke="#666" stroke-width="1.2"/>"##,
                        ),
                        tx = n(cx),
                        ty = n(ccy - r),
                    );
                }
            }
            actor_label(out, cx, y + h - 8.0, lines, "actor actor-box");
        }
    }
    if !top {
        out.push_str("</g>");
    }
}

/// Emit an actor/participant label (`byTspan`): one `<text>` per `<br>` line,
/// all at the box centre `cy`, with each `<tspan>` vertically centred via
/// `dy = i*16 - 16*(lines-1)/2`.
fn actor_label(out: &mut String, cx: f64, cy: f64, lines: &[String], class: &str) {
    let count = lines.len().max(1) as f64;
    for (i, line) in lines.iter().enumerate() {
        let dy = i as f64 * 16.0 - 16.0 * (count - 1.0) / 2.0;
        let _ = write!(
            out,
            concat!(
                r#"<text x="{cx}" y="{cy}" dominant-baseline="central" alignment-baseline="central" "#,
                r#"class="{class}" style="text-anchor: middle; font-size: 16px; font-weight: 400;">"#,
                r#"<tspan x="{cx}" dy="{dy}">{t}</tspan></text>"#,
            ),
            cx = n(cx),
            cy = n(cy),
            class = class,
            dy = n(dy),
            t = esc(line_or_zwsp(line)),
        );
    }
}

/// A pre-message element (note or control-structure block), for event-ordered
/// emission.
enum PreElem<'a> {
    Note(&'a crate::layout::sequence::PlacedNote),
    Block(&'a crate::layout::sequence::PlacedBlock),
    Activation(&'a crate::layout::sequence::PlacedActivation),
}

/// Emit an activation bar: `<g><rect class="activationN"/></g>`.
fn activation_bar(out: &mut String, a: &crate::layout::sequence::PlacedActivation) {
    let _ = write!(
        out,
        concat!(
            r##"<g><rect x="{x}" y="{y}" fill="#EDF2AE" stroke="#666" width="{w}" height="{h}" "##,
            r#"class="activation{c}"/></g>"#,
        ),
        x = n(a.x),
        y = n(a.y),
        w = n(a.w),
        h = n(a.h),
        c = a.class_idx,
    );
}

/// Emit a control-structure box: 4 `loopLine`s, section dividers, the corner
/// `labelBox` polygon + `labelText`, the `loopText` condition, and section
/// titles — matching mermaid's `drawLoop`.
fn block_box(out: &mut String, b: &crate::layout::sequence::PlacedBlock) {
    let _ = write!(
        out,
        r#"<g data-et="control-structure" data-id="i{}">"#,
        b.id
    );
    let line = |out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64, dashed: bool| {
        let style = if dashed {
            r#" style="stroke-dasharray: 3, 3;""#
        } else {
            ""
        };
        let _ = write!(
            out,
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" class="loopLine"{}/>"#,
            n(x1),
            n(y1),
            n(x2),
            n(y2),
            style,
        );
    };
    // Box border: top, right, bottom, left.
    line(out, b.startx, b.starty, b.stopx, b.starty, false);
    line(out, b.stopx, b.starty, b.stopx, b.stopy, false);
    line(out, b.startx, b.stopy, b.stopx, b.stopy, false);
    line(out, b.startx, b.starty, b.startx, b.stopy, false);
    // Section dividers (else/and).
    for (y, _) in &b.sections {
        line(out, b.startx, *y, b.stopx, *y, true);
    }
    // Corner label tab + keyword.
    let _ = write!(
        out,
        r#"<polygon points="{}" class="labelBox"/>"#,
        genpoints(b.startx, b.starty, 50.0, 20.0, 7.0),
    );
    let _ = write!(
        out,
        concat!(
            r#"<text x="{x}" y="{y}" text-anchor="middle" dominant-baseline="middle" "#,
            r#"alignment-baseline="middle" class="labelText" style="font-size: 16px; font-weight: 400;">{t}</text>"#,
        ),
        x = n((b.startx + 25.0).round()),
        y = n((b.starty + 12.5).round()),
        t = esc(&b.label),
    );
    // Condition text.
    if !b.title.trim().is_empty() {
        let cx = b.startx + 25.0 + (b.stopx - b.startx) / 2.0;
        let _ = write!(
            out,
            concat!(
                r#"<text x="{x}" y="{y}" text-anchor="middle" class="loopText" "#,
                r#"style="font-size: 16px; font-weight: 400;"><tspan x="{x}">[{t}]</tspan></text>"#,
            ),
            x = n(cx),
            y = n((b.starty + 17.5).round()),
            t = esc(&b.title),
        );
    }
    // Section titles (else/and conditions).
    for (y, title) in &b.sections {
        if title.trim().is_empty() {
            continue;
        }
        let cx = b.startx + (b.stopx - b.startx) / 2.0;
        let _ = write!(
            out,
            concat!(
                r#"<text x="{x}" y="{y}" text-anchor="middle" class="sectionTitle" "#,
                r#"style="font-size: 16px; font-weight: 400;">[{t}]</text>"#,
            ),
            x = n(cx),
            y = n((y + 17.5).round()),
            t = esc(title),
        );
    }
    out.push_str("</g>");
}

/// mermaid's `genPoints` for the label tab polygon (cut corner).
fn genpoints(x: f64, y: f64, w: f64, h: f64, cut: f64) -> String {
    format!(
        "{},{} {},{} {},{} {},{} {},{}",
        n(x),
        n(y),
        n(x + w),
        n(y),
        n(x + w),
        n(y + h - cut),
        n(x + w - cut * 1.2),
        n(y + h),
        n(x),
        n(y + h),
    )
}

/// Emit a note: `<g data-et="note">` wrapping a `note` rect + `noteText`.
fn note_box(out: &mut String, note: &crate::layout::sequence::PlacedNote) {
    let cx = note.x + note.width / 2.0;
    let _ = write!(out, r#"<g data-et="note" data-id="i{id}">"#, id = note.id,);
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
    // One `<text>` (with a `<tspan>`) per display line, stepping y.
    for (i, line) in note.lines.iter().enumerate() {
        let _ = write!(
            out,
            concat!(
                r#"<text x="{cx}" y="{ty}" text-anchor="middle" dominant-baseline="middle" "#,
                r#"alignment-baseline="middle" class="noteText" dy="1em" "#,
                r#"style="font-size: 16px; font-weight: 400;"><tspan x="{cx}">{t}</tspan></text>"#,
            ),
            cx = n(cx),
            ty = n(note.y + 5.0 + i as f64 * SEQ_LINE_H),
            t = esc(line_or_zwsp(line)),
        );
    }
    out.push_str("</g>");
}

/// Emit one message: label `<text>`, arrow `<line>`, and autonumber elements.
fn message(out: &mut String, m: &PlacedMessage, actors: &[crate::layout::sequence::PlacedActor]) {
    let (lo, hi) = (m.start_x.min(m.stop_x), m.start_x.max(m.stop_x));
    let text_x = (lo + (hi - lo) / 2.0).round();
    // One `<text>` per `<br>`-separated line, stepping y (mermaid uses separate
    // texts for messageText, not tspans).
    for (i, line) in split_lines(&m.text).iter().enumerate() {
        let _ = write!(
            out,
            concat!(
                r#"<text x="{x}" y="{y}" text-anchor="middle" dominant-baseline="middle" "#,
                r#"alignment-baseline="middle" style="font-size: 16px; font-weight: 400;" "#,
                r#"class="messageText" dy="1em">{t}</text>"#,
            ),
            x = n(text_x),
            y = n(m.text_y + i as f64 * SEQ_LINE_H),
            t = esc(line_or_zwsp(line)),
        );
    }

    let dotted = m.arrow.is_dotted();
    let class = if dotted {
        "messageLine1"
    } else {
        "messageLine0"
    };
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
    let start_marker = match m.arrow {
        SeqArrow::BiSolid | SeqArrow::BiDotted => Some("arrowhead"),
        // Reverse directional arrows put the head at the source (the opposite
        // top/bottom head).
        SeqArrow::SolidTopRev | SeqArrow::SolidTopRevDotted => Some("solidBottomArrowHead"),
        SeqArrow::SolidBottomRev | SeqArrow::SolidBottomRevDotted => Some("solidTopArrowHead"),
        SeqArrow::StickTopRev | SeqArrow::StickTopRevDotted => Some("stickBottomArrowHead"),
        SeqArrow::StickBottomRev | SeqArrow::StickBottomRevDotted => Some("stickTopArrowHead"),
        _ => None,
    };
    let marker_start = match start_marker {
        Some(mk) => format!(r#" marker-start="url(#{ID}-{mk})""#),
        None => String::new(),
    };
    if m.self_loop {
        // Cubic loop bulging right: M sx,Y C sx+60,Y-10 sx+60,Y+30 sx,Y+20.
        let sx = m.start_x;
        let y = m.line_y;
        let _ = write!(
            out,
            concat!(
                r#"<path d="M {sx},{y} C {c1x},{y1} {c1x},{y2} {sx},{y3}" class="{class}" "#,
                r#"data-et="message" data-id="i{idx}" data-from="{from}" data-to="{to}" "#,
                r#"stroke-width="2" stroke="none"{me}{style}/>"#,
            ),
            sx = n(sx),
            y = n(y),
            c1x = n(sx + 60.0),
            y1 = n(y - 10.0),
            y2 = n(y + 30.0),
            y3 = n(y + 20.0),
            class = class,
            idx = m.id,
            from = esc(&actors[m.from].id),
            to = esc(&actors[m.to].id),
            me = marker_end,
            style = style,
        );
        return;
    }
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
        let ax = m.start_x;
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
        // Directional (non-reverse) arrows point at the target.
        SeqArrow::SolidTop | SeqArrow::SolidTopDotted => Some("solidTopArrowHead"),
        SeqArrow::SolidBottom | SeqArrow::SolidBottomDotted => Some("solidBottomArrowHead"),
        SeqArrow::StickTop | SeqArrow::StickTopDotted => Some("stickTopArrowHead"),
        SeqArrow::StickBottom | SeqArrow::StickBottomDotted => Some("stickBottomArrowHead"),
        // Open + reverse directional arrows draw no end marker.
        SeqArrow::SolidOpen
        | SeqArrow::DottedOpen
        | SeqArrow::SolidTopRev
        | SeqArrow::SolidBottomRev
        | SeqArrow::SolidTopRevDotted
        | SeqArrow::SolidBottomRevDotted
        | SeqArrow::StickTopRev
        | SeqArrow::StickBottomRev
        | SeqArrow::StickTopRevDotted
        | SeqArrow::StickBottomRevDotted => None,
    }
}

/// The sequence `<style>` block — mermaid's default-theme CSS verbatim. These
/// stylesheet rules override the elements' presentation attributes (lifeline
/// stroke, loop-line dash/colour, activation fill, all-`line` width), so
/// emitting them is what makes the render match the oracle visually. Structural
/// comparison ignores `<style>` text.
fn style_block() -> String {
    format!("<style>{}</style>", seq_defs::SEQ_STYLE)
}

/// Per-line height for stacked multi-line labels (mermaid's rounded bbox at 16px).
const SEQ_LINE_H: f64 = 19.0;

/// Split a label on `<br>`/`<br/>`/newlines into display lines, then decode
/// mermaid's `#…;` entity escapes per line.
/// mermaid renders an empty label line (from a trailing or consecutive `<br/>`)
/// as a zero-width space, so the `<text>`/`<tspan>` keeps its line height.
fn line_or_zwsp(line: &str) -> &str {
    if line.is_empty() {
        "\u{200b}"
    } else {
        line
    }
}

fn split_lines(text: &str) -> Vec<String> {
    text.replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .split('\n')
        .map(|s| crate::text::decode_entities(s.trim()))
        .collect()
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
