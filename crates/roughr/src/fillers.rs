//! Port of rough.js's hachure fill path: `fillers/filler.js` (only the
//! `hachure` filler is ported), `fillers/hachure-filler.js`,
//! `fillers/scan-line-hachure.js`, and the `hachure-fill` npm package's
//! `hachureLines` scan-line algorithm.
//!
//! Other fill styles (zigzag, cross-hatch, dots, dashed, ...) are intentionally
//! omitted — mermaid only uses `solid` and `hachure`.

use crate::core::{OpSet, OpSetType, Options, Point};
use crate::renderer::{double_line_fill_ops, random};

/// rough.js `polygonHachureLines` + `HachureFiller._fillPolygons` +
/// `renderLines`, producing a `fillSketch` OpSet.
pub fn hachure_fill_polygons(polygon_list: &[Vec<Point>], o: &mut Options) -> OpSet {
    let lines = polygon_hachure_lines(polygon_list, o);
    let mut ops = Vec::new();
    for l in &lines {
        ops.extend(double_line_fill_ops(l[0][0], l[0][1], l[1][0], l[1][1], o));
    }
    OpSet::new(OpSetType::FillSketch, ops)
}

/// rough.js `polygonHachureLines`.
fn polygon_hachure_lines(polygon_list: &[Vec<Point>], o: &mut Options) -> Vec<[Point; 2]> {
    let angle = o.hachure_angle + 90.0;
    let mut gap = o.hachure_gap;
    if gap < 0.0 {
        gap = o.stroke_width * 4.0;
    }
    gap = js_round(gap.max(0.1));

    let mut skip_offset = 1.0;
    if o.roughness >= 1.0 {
        // rough.js: (o.randomizer?.next() || Math.random()) > 0.7
        // We have no Math.random; use the shared randomizer (creating it if
        // necessary). Deterministic by construction.
        if random(o) > 0.7 {
            skip_offset = gap;
        }
    }

    let step = if skip_offset != 0.0 { skip_offset } else { 1.0 };
    hachure_lines(polygon_list, gap, angle, step)
}

fn js_round(x: f64) -> f64 {
    // JS Math.round: floor(x + 0.5)
    (x + 0.5).floor()
}

fn rotate_points(points: &mut [Point], center: Point, degrees: f64) {
    if points.is_empty() {
        return;
    }
    let [cx, cy] = center;
    let angle = (std::f64::consts::PI / 180.0) * degrees;
    let cos = angle.cos();
    let sin = angle.sin();
    for p in points.iter_mut() {
        let x = p[0];
        let y = p[1];
        p[0] = ((x - cx) * cos) - ((y - cy) * sin) + cx;
        p[1] = ((x - cx) * sin) + ((y - cy) * cos) + cy;
    }
}

fn rotate_lines(lines: &mut [[Point; 2]], center: Point, degrees: f64) {
    let mut pts: Vec<Point> = Vec::with_capacity(lines.len() * 2);
    for l in lines.iter() {
        pts.push(l[0]);
        pts.push(l[1]);
    }
    rotate_points(&mut pts, center, degrees);
    for (i, l) in lines.iter_mut().enumerate() {
        l[0] = pts[i * 2];
        l[1] = pts[i * 2 + 1];
    }
}

fn are_same_points(p1: Point, p2: Point) -> bool {
    p1[0] == p2[0] && p1[1] == p2[1]
}

/// Port of the `hachure-fill` npm package `hachureLines`.
fn hachure_lines(
    polygons: &[Vec<Point>],
    hachure_gap: f64,
    hachure_angle: f64,
    hachure_step_offset: f64,
) -> Vec<[Point; 2]> {
    let angle = hachure_angle;
    let gap = hachure_gap.max(0.1);
    // Work on an owned, mutable copy (rough.js rotates in place).
    let mut polygon_list: Vec<Vec<Point>> = polygons.to_vec();
    let rotation_center: Point = [0.0, 0.0];
    if angle != 0.0 {
        for polygon in polygon_list.iter_mut() {
            rotate_points(polygon, rotation_center, angle);
        }
    }
    let mut lines = straight_hachure_lines(&polygon_list, gap, hachure_step_offset);
    if angle != 0.0 {
        rotate_lines(&mut lines, rotation_center, -angle);
    }
    lines
}

#[derive(Clone, Copy)]
struct Edge {
    ymin: f64,
    ymax: f64,
    x: f64,
    islope: f64,
}

#[allow(clippy::collapsible_if)] // kept nested to mirror the rough.js structure
fn straight_hachure_lines(
    polygons: &[Vec<Point>],
    gap_in: f64,
    hachure_step_offset: f64,
) -> Vec<[Point; 2]> {
    let mut vertex_array: Vec<Vec<Point>> = Vec::new();
    for polygon in polygons {
        let mut vertices = polygon.clone();
        if vertices.len() >= 2 {
            let first = vertices[0];
            let last = vertices[vertices.len() - 1];
            if !are_same_points(first, last) {
                vertices.push([first[0], first[1]]);
            }
        }
        if vertices.len() > 2 {
            vertex_array.push(vertices);
        }
    }

    let mut lines: Vec<[Point; 2]> = Vec::new();
    let gap = gap_in.max(0.1);

    let mut edges: Vec<Edge> = Vec::new();
    for vertices in &vertex_array {
        for i in 0..vertices.len() - 1 {
            let p1 = vertices[i];
            let p2 = vertices[i + 1];
            if p1[1] != p2[1] {
                let ymin = p1[1].min(p2[1]);
                edges.push(Edge {
                    ymin,
                    ymax: p1[1].max(p2[1]),
                    x: if ymin == p1[1] { p1[0] } else { p2[0] },
                    islope: (p2[0] - p1[0]) / (p2[1] - p1[1]),
                });
            }
        }
    }

    edges.sort_by(|e1, e2| {
        e1.ymin
            .partial_cmp(&e2.ymin)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(e1.x.partial_cmp(&e2.x).unwrap_or(std::cmp::Ordering::Equal))
            .then(
                e1.ymax
                    .partial_cmp(&e2.ymax)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    if edges.is_empty() {
        return lines;
    }

    let mut active: Vec<Edge> = Vec::new();
    let mut y = edges[0].ymin;
    let mut iteration: i64 = 0;

    while !active.is_empty() || !edges.is_empty() {
        if !edges.is_empty() {
            let mut ix: i64 = -1;
            for (i, e) in edges.iter().enumerate() {
                if e.ymin > y {
                    break;
                }
                ix = i as i64;
            }
            let count = (ix + 1) as usize;
            let removed: Vec<Edge> = edges.drain(0..count).collect();
            for e in removed {
                active.push(e);
            }
        }

        active.retain(|e| e.ymax > y);
        active.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

        if (hachure_step_offset != 1.0) || ((iteration as f64) % gap == 0.0) {
            if active.len() > 1 {
                let mut i = 0;
                while i < active.len() {
                    let nexti = i + 1;
                    if nexti >= active.len() {
                        break;
                    }
                    let ce = &active[i];
                    let ne = &active[nexti];
                    lines.push([[js_round(ce.x), y], [js_round(ne.x), y]]);
                    i += 2;
                }
            }
        }

        y += hachure_step_offset;
        for e in active.iter_mut() {
            e.x += hachure_step_offset * e.islope;
        }
        iteration += 1;
    }

    lines
}
