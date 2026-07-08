//! Port of rough.js `renderer.js`: the low-level op generators that turn
//! primitive shapes into `OpSet`s of hand-drawn `move`/`bcurveTo`/`lineTo` ops.
//!
//! Every function that consumes randomness threads `&mut Options` so that the
//! lazily-created `randomizer` (see [`crate::core::Options`]) advances through a
//! single deterministic sequence per shape, exactly like rough.js.

use crate::core::{Op, OpSet, OpSetType, OpType, Options, Point};
use crate::fillers;
use crate::math::Random;
use crate::pathdata::{absolutize, normalize, parse_path};
use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Randomness helpers (rough.js `random`, `_offset`, `_offsetOpt`)
// ---------------------------------------------------------------------------

/// rough.js `random(ops)` — lazily create `Random(seed || 0)` and advance it.
pub(crate) fn random(o: &mut Options) -> f64 {
    if o.randomizer.is_none() {
        o.randomizer = Some(Random::new(o.seed));
    }
    o.randomizer.as_mut().unwrap().next()
}

/// rough.js `_offset(min, max, ops, roughnessGain = 1)`.
fn offset(min: f64, max: f64, o: &mut Options, roughness_gain: f64) -> f64 {
    o.roughness * roughness_gain * ((random(o) * (max - min)) + min)
}

/// rough.js `_offsetOpt(x, ops, roughnessGain = 1)`.
pub(crate) fn offset_opt(x: f64, o: &mut Options, roughness_gain: f64) -> f64 {
    offset(-x, x, o, roughness_gain)
}

/// rough.js `cloneOptionsAlterSeed(ops)`.
fn clone_options_alter_seed(o: &Options) -> Options {
    let mut result = o.clone();
    result.randomizer = None;
    if o.seed != 0 {
        result.seed = o.seed.wrapping_add(1);
    }
    result
}

// ---------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------

/// rough.js `_line`.
fn line_ops(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &mut Options,
    move_: bool,
    overlay: bool,
) -> Vec<Op> {
    let length_sq = (x1 - x2).powi(2) + (y1 - y2).powi(2);
    let length = length_sq.sqrt();
    let roughness_gain = if length < 200.0 {
        1.0
    } else if length > 500.0 {
        0.4
    } else {
        (-0.0016668) * length + 1.233334
    };

    let mut off = o.max_randomness_offset;
    if (off * off * 100.0) > length_sq {
        off = length / 10.0;
    }
    let half_offset = off / 2.0;
    let diverge_point = 0.2 + random(o) * 0.2;
    let mut mid_disp_x = o.bowing * o.max_randomness_offset * (y2 - y1) / 200.0;
    let mut mid_disp_y = o.bowing * o.max_randomness_offset * (x1 - x2) / 200.0;
    mid_disp_x = offset_opt(mid_disp_x, o, roughness_gain);
    mid_disp_y = offset_opt(mid_disp_y, o, roughness_gain);

    let preserve = o.preserve_vertices;
    let mut ops = Vec::new();

    if move_ {
        if overlay {
            let mx = x1
                + if preserve {
                    0.0
                } else {
                    offset_opt(half_offset, o, roughness_gain)
                };
            let my = y1
                + if preserve {
                    0.0
                } else {
                    offset_opt(half_offset, o, roughness_gain)
                };
            ops.push(Op::new(OpType::Move, vec![mx, my]));
        } else {
            let mx = x1
                + if preserve {
                    0.0
                } else {
                    offset_opt(off, o, roughness_gain)
                };
            let my = y1
                + if preserve {
                    0.0
                } else {
                    offset_opt(off, o, roughness_gain)
                };
            ops.push(Op::new(OpType::Move, vec![mx, my]));
        }
    }

    if overlay {
        let d0 = mid_disp_x
            + x1
            + (x2 - x1) * diverge_point
            + offset_opt(half_offset, o, roughness_gain);
        let d1 = mid_disp_y
            + y1
            + (y2 - y1) * diverge_point
            + offset_opt(half_offset, o, roughness_gain);
        let d2 = mid_disp_x
            + x1
            + 2.0 * (x2 - x1) * diverge_point
            + offset_opt(half_offset, o, roughness_gain);
        let d3 = mid_disp_y
            + y1
            + 2.0 * (y2 - y1) * diverge_point
            + offset_opt(half_offset, o, roughness_gain);
        let d4 = x2
            + if preserve {
                0.0
            } else {
                offset_opt(half_offset, o, roughness_gain)
            };
        let d5 = y2
            + if preserve {
                0.0
            } else {
                offset_opt(half_offset, o, roughness_gain)
            };
        ops.push(Op::new(OpType::BCurveTo, vec![d0, d1, d2, d3, d4, d5]));
    } else {
        let d0 = mid_disp_x + x1 + (x2 - x1) * diverge_point + offset_opt(off, o, roughness_gain);
        let d1 = mid_disp_y + y1 + (y2 - y1) * diverge_point + offset_opt(off, o, roughness_gain);
        let d2 =
            mid_disp_x + x1 + 2.0 * (x2 - x1) * diverge_point + offset_opt(off, o, roughness_gain);
        let d3 =
            mid_disp_y + y1 + 2.0 * (y2 - y1) * diverge_point + offset_opt(off, o, roughness_gain);
        let d4 = x2
            + if preserve {
                0.0
            } else {
                offset_opt(off, o, roughness_gain)
            };
        let d5 = y2
            + if preserve {
                0.0
            } else {
                offset_opt(off, o, roughness_gain)
            };
        ops.push(Op::new(OpType::BCurveTo, vec![d0, d1, d2, d3, d4, d5]));
    }
    ops
}

/// rough.js `_doubleLine`.
fn double_line(x1: f64, y1: f64, x2: f64, y2: f64, o: &mut Options, filling: bool) -> Vec<Op> {
    let single_stroke = if filling {
        o.disable_multi_stroke_fill
    } else {
        o.disable_multi_stroke
    };
    let mut o1 = line_ops(x1, y1, x2, y2, o, true, false);
    if single_stroke {
        return o1;
    }
    let o2 = line_ops(x1, y1, x2, y2, o, true, true);
    o1.extend(o2);
    o1
}

/// rough.js `doubleLineFillOps` (used by fillers).
pub(crate) fn double_line_fill_ops(x1: f64, y1: f64, x2: f64, y2: f64, o: &mut Options) -> Vec<Op> {
    double_line(x1, y1, x2, y2, o, true)
}

/// rough.js `line`.
pub fn line(x1: f64, y1: f64, x2: f64, y2: f64, o: &mut Options) -> OpSet {
    OpSet::new(OpSetType::Path, double_line(x1, y1, x2, y2, o, false))
}

/// rough.js `linearPath`.
pub fn linear_path(points: &[Point], close: bool, o: &mut Options) -> OpSet {
    let len = points.len();
    if len > 2 {
        let mut ops = Vec::new();
        for i in 0..(len - 1) {
            ops.extend(double_line(
                points[i][0],
                points[i][1],
                points[i + 1][0],
                points[i + 1][1],
                o,
                false,
            ));
        }
        if close {
            ops.extend(double_line(
                points[len - 1][0],
                points[len - 1][1],
                points[0][0],
                points[0][1],
                o,
                false,
            ));
        }
        OpSet::new(OpSetType::Path, ops)
    } else if len == 2 {
        line(points[0][0], points[0][1], points[1][0], points[1][1], o)
    } else {
        OpSet::new(OpSetType::Path, Vec::new())
    }
}

/// rough.js `polygon`.
pub fn polygon(points: &[Point], o: &mut Options) -> OpSet {
    linear_path(points, true, o)
}

/// rough.js `rectangle`.
pub fn rectangle(x: f64, y: f64, width: f64, height: f64, o: &mut Options) -> OpSet {
    let points = [
        [x, y],
        [x + width, y],
        [x + width, y + height],
        [x, y + height],
    ];
    polygon(&points, o)
}

// ---------------------------------------------------------------------------
// Curves
// ---------------------------------------------------------------------------

/// rough.js `_curveWithOffset`.
fn curve_with_offset(points: &[Point], off: f64, o: &mut Options) -> Vec<Op> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut ps: Vec<Point> = Vec::new();
    ps.push([
        points[0][0] + offset_opt(off, o, 1.0),
        points[0][1] + offset_opt(off, o, 1.0),
    ]);
    ps.push([
        points[0][0] + offset_opt(off, o, 1.0),
        points[0][1] + offset_opt(off, o, 1.0),
    ]);
    for i in 1..points.len() {
        ps.push([
            points[i][0] + offset_opt(off, o, 1.0),
            points[i][1] + offset_opt(off, o, 1.0),
        ]);
        if i == points.len() - 1 {
            ps.push([
                points[i][0] + offset_opt(off, o, 1.0),
                points[i][1] + offset_opt(off, o, 1.0),
            ]);
        }
    }
    curve_ops(&ps, None, o)
}

/// rough.js `_curve`.
fn curve_ops(points: &[Point], close_point: Option<Point>, o: &mut Options) -> Vec<Op> {
    let len = points.len();
    let mut ops = Vec::new();
    if len > 3 {
        let s = 1.0 - o.curve_tightness;
        ops.push(Op::new(OpType::Move, vec![points[1][0], points[1][1]]));
        let mut i = 1;
        while i + 2 < len {
            let cached = points[i];
            let b0 = [cached[0], cached[1]];
            let b1 = [
                cached[0] + (s * points[i + 1][0] - s * points[i - 1][0]) / 6.0,
                cached[1] + (s * points[i + 1][1] - s * points[i - 1][1]) / 6.0,
            ];
            let b2 = [
                points[i + 1][0] + (s * points[i][0] - s * points[i + 2][0]) / 6.0,
                points[i + 1][1] + (s * points[i][1] - s * points[i + 2][1]) / 6.0,
            ];
            let b3 = [points[i + 1][0], points[i + 1][1]];
            let _ = b0; // parity with rough.js (b[0] computed but unused in the op)
            ops.push(Op::new(
                OpType::BCurveTo,
                vec![b1[0], b1[1], b2[0], b2[1], b3[0], b3[1]],
            ));
            i += 1;
        }
        if let Some(cp) = close_point {
            let ro = o.max_randomness_offset;
            let lx = cp[0] + offset_opt(ro, o, 1.0);
            let ly = cp[1] + offset_opt(ro, o, 1.0);
            ops.push(Op::new(OpType::LineTo, vec![lx, ly]));
        }
    } else if len == 3 {
        ops.push(Op::new(OpType::Move, vec![points[1][0], points[1][1]]));
        ops.push(Op::new(
            OpType::BCurveTo,
            vec![
                points[1][0],
                points[1][1],
                points[2][0],
                points[2][1],
                points[2][0],
                points[2][1],
            ],
        ));
    } else if len == 2 {
        ops.extend(line_ops(
            points[0][0],
            points[0][1],
            points[1][0],
            points[1][1],
            o,
            true,
            true,
        ));
    }
    ops
}

/// rough.js `curve`. Accepts a single point list.
pub fn curve(input_points: &[Point], o: &mut Options) -> OpSet {
    if input_points.is_empty() {
        return OpSet::new(OpSetType::Path, Vec::new());
    }
    let mut o1 = curve_with_offset(input_points, 1.0 * (1.0 + o.roughness * 0.2), o);
    let o2 = if o.disable_multi_stroke {
        Vec::new()
    } else {
        let mut alt = clone_options_alter_seed(o);
        curve_with_offset(input_points, 1.5 * (1.0 + o.roughness * 0.22), &mut alt)
    };
    o1.extend(o2);
    OpSet::new(OpSetType::Path, o1)
}

// ---------------------------------------------------------------------------
// Ellipse / arc
// ---------------------------------------------------------------------------

/// rough.js `EllipseParams`.
#[derive(Clone, Copy, Debug)]
pub struct EllipseParams {
    pub increment: f64,
    pub rx: f64,
    pub ry: f64,
}

/// Result of [`ellipse_with_params`]: rough.js `EllipseResult`.
pub struct EllipseResult {
    pub estimated_points: Vec<Point>,
    pub opset: OpSet,
}

/// rough.js `generateEllipseParams`.
pub fn generate_ellipse_params(width: f64, height: f64, o: &mut Options) -> EllipseParams {
    let psq = (PI * 2.0 * (((width / 2.0).powi(2) + (height / 2.0).powi(2)) / 2.0).sqrt()).sqrt();
    let step_count = (o
        .curve_step_count
        .max((o.curve_step_count / (200.0f64).sqrt()) * psq))
    .ceil();
    let increment = (PI * 2.0) / step_count;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    let curve_fit_randomness = 1.0 - o.curve_fitting;
    rx += offset_opt(rx * curve_fit_randomness, o, 1.0);
    ry += offset_opt(ry * curve_fit_randomness, o, 1.0);
    EllipseParams { increment, rx, ry }
}

/// rough.js `ellipseWithParams`.
pub fn ellipse_with_params(x: f64, y: f64, o: &mut Options, p: &EllipseParams) -> EllipseResult {
    // overlap arg: increment * _offset(0.1, _offset(0.4, 1, o), o)
    let inner = offset(0.4, 1.0, o, 1.0);
    let overlap = p.increment * offset(0.1, inner, o, 1.0);
    let (ap1, cp1) = compute_ellipse_points(p.increment, x, y, p.rx, p.ry, 1.0, overlap, o);
    let mut o1 = curve_ops(&ap1, None, o);
    if (!o.disable_multi_stroke) && (o.roughness != 0.0) {
        let (ap2, _) = compute_ellipse_points(p.increment, x, y, p.rx, p.ry, 1.5, 0.0, o);
        let o2 = curve_ops(&ap2, None, o);
        o1.extend(o2);
    }
    EllipseResult {
        estimated_points: cp1,
        opset: OpSet::new(OpSetType::Path, o1),
    }
}

/// rough.js `ellipse`.
pub fn ellipse(x: f64, y: f64, width: f64, height: f64, o: &mut Options) -> OpSet {
    let params = generate_ellipse_params(width, height, o);
    ellipse_with_params(x, y, o, &params).opset
}

/// rough.js `_computeEllipsePoints`. Returns `(allPoints, corePoints)`.
#[allow(clippy::too_many_arguments)]
fn compute_ellipse_points(
    mut increment: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    off: f64,
    overlap: f64,
    o: &mut Options,
) -> (Vec<Point>, Vec<Point>) {
    let core_only = o.roughness == 0.0;
    let mut core_points: Vec<Point> = Vec::new();
    let mut all_points: Vec<Point> = Vec::new();

    if core_only {
        increment /= 4.0;
        all_points.push([cx + rx * (-increment).cos(), cy + ry * (-increment).sin()]);
        let mut angle = 0.0;
        while angle <= PI * 2.0 {
            let p = [cx + rx * angle.cos(), cy + ry * angle.sin()];
            core_points.push(p);
            all_points.push(p);
            angle += increment;
        }
        all_points.push([cx + rx * (0.0f64).cos(), cy + ry * (0.0f64).sin()]);
        all_points.push([cx + rx * increment.cos(), cy + ry * increment.sin()]);
    } else {
        let rad_offset = offset_opt(0.5, o, 1.0) - (PI / 2.0);
        // First point (two offset_opt calls, x then y).
        let fx = offset_opt(off, o, 1.0) + cx + 0.9 * rx * (rad_offset - increment).cos();
        let fy = offset_opt(off, o, 1.0) + cy + 0.9 * ry * (rad_offset - increment).sin();
        all_points.push([fx, fy]);

        let end_angle = PI * 2.0 + rad_offset - 0.01;
        let mut angle = rad_offset;
        while angle < end_angle {
            let px = offset_opt(off, o, 1.0) + cx + rx * angle.cos();
            let py = offset_opt(off, o, 1.0) + cy + ry * angle.sin();
            let p = [px, py];
            core_points.push(p);
            all_points.push(p);
            angle += increment;
        }

        let a1x = offset_opt(off, o, 1.0) + cx + rx * (rad_offset + PI * 2.0 + overlap * 0.5).cos();
        let a1y = offset_opt(off, o, 1.0) + cy + ry * (rad_offset + PI * 2.0 + overlap * 0.5).sin();
        all_points.push([a1x, a1y]);

        let a2x = offset_opt(off, o, 1.0) + cx + 0.98 * rx * (rad_offset + overlap).cos();
        let a2y = offset_opt(off, o, 1.0) + cy + 0.98 * ry * (rad_offset + overlap).sin();
        all_points.push([a2x, a2y]);

        let a3x = offset_opt(off, o, 1.0) + cx + 0.9 * rx * (rad_offset + overlap * 0.5).cos();
        let a3y = offset_opt(off, o, 1.0) + cy + 0.9 * ry * (rad_offset + overlap * 0.5).sin();
        all_points.push([a3x, a3y]);
    }

    (all_points, core_points)
}

/// rough.js `_arc`.
#[allow(clippy::too_many_arguments)]
fn arc_ops(
    increment: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    strt: f64,
    stp: f64,
    off: f64,
    o: &mut Options,
) -> Vec<Op> {
    let rad_offset = strt + offset_opt(0.1, o, 1.0);
    let mut points: Vec<Point> = Vec::new();
    let p0x = offset_opt(off, o, 1.0) + cx + 0.9 * rx * (rad_offset - increment).cos();
    let p0y = offset_opt(off, o, 1.0) + cy + 0.9 * ry * (rad_offset - increment).sin();
    points.push([p0x, p0y]);
    let mut angle = rad_offset;
    while angle <= stp {
        let px = offset_opt(off, o, 1.0) + cx + rx * angle.cos();
        let py = offset_opt(off, o, 1.0) + cy + ry * angle.sin();
        points.push([px, py]);
        angle += increment;
    }
    points.push([cx + rx * stp.cos(), cy + ry * stp.sin()]);
    points.push([cx + rx * stp.cos(), cy + ry * stp.sin()]);
    curve_ops(&points, None, o)
}

/// rough.js `arc`.
#[allow(clippy::too_many_arguments)]
pub fn arc(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    start: f64,
    stop: f64,
    closed: bool,
    rough_closure: bool,
    o: &mut Options,
) -> OpSet {
    let cx = x;
    let cy = y;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    rx += offset_opt(rx * 0.01, o, 1.0);
    ry += offset_opt(ry * 0.01, o, 1.0);
    let mut strt = start;
    let mut stp = stop;
    while strt < 0.0 {
        strt += PI * 2.0;
        stp += PI * 2.0;
    }
    if (stp - strt) > (PI * 2.0) {
        strt = 0.0;
        stp = PI * 2.0;
    }
    let ellipse_inc = (PI * 2.0) / o.curve_step_count;
    let arc_inc = (ellipse_inc / 2.0).min((stp - strt) / 2.0);
    let mut ops = arc_ops(arc_inc, cx, cy, rx, ry, strt, stp, 1.0, o);
    if !o.disable_multi_stroke {
        let o2 = arc_ops(arc_inc, cx, cy, rx, ry, strt, stp, 1.5, o);
        ops.extend(o2);
    }
    if closed {
        if rough_closure {
            ops.extend(double_line(
                cx,
                cy,
                cx + rx * strt.cos(),
                cy + ry * strt.sin(),
                o,
                false,
            ));
            ops.extend(double_line(
                cx,
                cy,
                cx + rx * stp.cos(),
                cy + ry * stp.sin(),
                o,
                false,
            ));
        } else {
            ops.push(Op::new(OpType::LineTo, vec![cx, cy]));
            ops.push(Op::new(
                OpType::LineTo,
                vec![cx + rx * strt.cos(), cy + ry * strt.sin()],
            ));
        }
    }
    OpSet::new(OpSetType::Path, ops)
}

// ---------------------------------------------------------------------------
// SVG path
// ---------------------------------------------------------------------------

/// rough.js `_bezierTo`.
#[allow(clippy::too_many_arguments)]
fn bezier_to(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x: f64,
    y: f64,
    current: Point,
    o: &mut Options,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let mr = if o.max_randomness_offset != 0.0 {
        o.max_randomness_offset
    } else {
        1.0
    };
    let ros = [mr, mr + 0.3];
    let iterations = if o.disable_multi_stroke { 1 } else { 2 };
    let preserve = o.preserve_vertices;
    for i in 0..iterations {
        if i == 0 {
            ops.push(Op::new(OpType::Move, vec![current[0], current[1]]));
        } else {
            let mx = current[0]
                + if preserve {
                    0.0
                } else {
                    offset_opt(ros[0], o, 1.0)
                };
            let my = current[1]
                + if preserve {
                    0.0
                } else {
                    offset_opt(ros[0], o, 1.0)
                };
            ops.push(Op::new(OpType::Move, vec![mx, my]));
        }
        let f = if preserve {
            [x, y]
        } else {
            [
                x + offset_opt(ros[i], o, 1.0),
                y + offset_opt(ros[i], o, 1.0),
            ]
        };
        let c0 = x1 + offset_opt(ros[i], o, 1.0);
        let c1 = y1 + offset_opt(ros[i], o, 1.0);
        let c2 = x2 + offset_opt(ros[i], o, 1.0);
        let c3 = y2 + offset_opt(ros[i], o, 1.0);
        ops.push(Op::new(OpType::BCurveTo, vec![c0, c1, c2, c3, f[0], f[1]]));
    }
    ops
}

/// rough.js `svgPath`.
pub fn svg_path(path: &str, o: &mut Options) -> OpSet {
    let segments = normalize(&absolutize(&parse_path(path)));
    let mut ops: Vec<Op> = Vec::new();
    let mut first: Point = [0.0, 0.0];
    let mut current: Point = [0.0, 0.0];
    for seg in &segments {
        let data = &seg.data;
        match seg.key {
            'M' => {
                current = [data[0], data[1]];
                first = [data[0], data[1]];
            }
            'L' => {
                ops.extend(double_line(
                    current[0], current[1], data[0], data[1], o, false,
                ));
                current = [data[0], data[1]];
            }
            'C' => {
                let x1 = data[0];
                let y1 = data[1];
                let x2 = data[2];
                let y2 = data[3];
                let x = data[4];
                let y = data[5];
                ops.extend(bezier_to(x1, y1, x2, y2, x, y, current, o));
                current = [x, y];
            }
            'Z' => {
                ops.extend(double_line(
                    current[0], current[1], first[0], first[1], o, false,
                ));
                current = [first[0], first[1]];
            }
            _ => {}
        }
    }
    OpSet::new(OpSetType::Path, ops)
}

// ---------------------------------------------------------------------------
// Fills
// ---------------------------------------------------------------------------

/// rough.js `solidFillPolygon`.
pub fn solid_fill_polygon(polygon_list: &[Vec<Point>], o: &mut Options) -> OpSet {
    let mut ops = Vec::new();
    for points in polygon_list {
        if !points.is_empty() {
            let off = o.max_randomness_offset;
            let len = points.len();
            if len > 2 {
                let mx = points[0][0] + offset_opt(off, o, 1.0);
                let my = points[0][1] + offset_opt(off, o, 1.0);
                ops.push(Op::new(OpType::Move, vec![mx, my]));
                for p in points.iter().skip(1) {
                    let lx = p[0] + offset_opt(off, o, 1.0);
                    let ly = p[1] + offset_opt(off, o, 1.0);
                    ops.push(Op::new(OpType::LineTo, vec![lx, ly]));
                }
            }
        }
    }
    OpSet::new(OpSetType::FillPath, ops)
}

/// rough.js `patternFillPolygons` (hachure fill only in this port).
pub fn pattern_fill_polygons(polygon_list: &[Vec<Point>], o: &mut Options) -> OpSet {
    fillers::hachure_fill_polygons(polygon_list, o)
}

/// rough.js `patternFillArc`.
pub fn pattern_fill_arc(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    start: f64,
    stop: f64,
    o: &mut Options,
) -> OpSet {
    let cx = x;
    let cy = y;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    rx += offset_opt(rx * 0.01, o, 1.0);
    ry += offset_opt(ry * 0.01, o, 1.0);
    let mut strt = start;
    let mut stp = stop;
    while strt < 0.0 {
        strt += PI * 2.0;
        stp += PI * 2.0;
    }
    if (stp - strt) > (PI * 2.0) {
        strt = 0.0;
        stp = PI * 2.0;
    }
    let increment = (stp - strt) / o.curve_step_count;
    let mut points: Vec<Point> = Vec::new();
    let mut angle = strt;
    while angle <= stp {
        points.push([cx + rx * angle.cos(), cy + ry * angle.sin()]);
        angle += increment;
    }
    points.push([cx + rx * stp.cos(), cy + ry * stp.sin()]);
    points.push([cx, cy]);
    pattern_fill_polygons(&[points], o)
}
