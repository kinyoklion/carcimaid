//! Port of the `path-data-parser` npm package (`parser.js`, `absolutize.js`,
//! `normalize.js`) that rough.js `svgPath` depends on.
//!
//! `normalize(absolutize(parsePath(d)))` reduces any SVG path `d` string to a
//! sequence of only `M`, `L`, `C`, and `Z` segments. Elliptical arcs (`A`/`a`)
//! are converted to cubic beziers via `arcToCubicCurves`, matching the original.

use std::f64::consts::PI;

/// A single parsed path segment.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    /// Command letter (e.g. `'M'`, `'l'`, `'C'`, `'z'`).
    pub key: char,
    pub data: Vec<f64>,
}

#[derive(Clone, Copy, PartialEq)]
enum TokKind {
    Command,
    Number,
}

struct Token {
    kind: TokKind,
    text: String,
}

fn params_for(key: char) -> Option<usize> {
    Some(match key {
        'A' | 'a' => 7,
        'C' | 'c' => 6,
        'H' | 'h' => 1,
        'L' | 'l' => 2,
        'M' | 'm' => 2,
        'Q' | 'q' => 4,
        'S' | 's' => 4,
        'T' | 't' => 2,
        'V' | 'v' => 1,
        'Z' | 'z' => 0,
        _ => return None,
    })
}

fn is_command_letter(c: char) -> bool {
    matches!(
        c,
        'a' | 'A'
            | 'c' | 'C'
            | 'h' | 'H'
            | 'l' | 'L'
            | 'm' | 'M'
            | 'q' | 'Q'
            | 's' | 'S'
            | 't' | 'T'
            | 'v' | 'V'
            | 'z' | 'Z'
    )
}

/// Manual reimplementation of the `path-data-parser` `tokenize` regex scanner.
///
/// Returns `None` when the input contains an unrecognized token (mirroring the
/// original returning `[]`).
fn tokenize(d: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = d.chars().collect();
    let n = chars.len();
    let mut i = 0usize;
    let mut tokens = Vec::new();
    while i < n {
        let c = chars[i];
        // Separators: [ \t\r\n,]
        if matches!(c, ' ' | '\t' | '\r' | '\n' | ',') {
            i += 1;
            continue;
        }
        if is_command_letter(c) {
            tokens.push(Token {
                kind: TokKind::Command,
                text: c.to_string(),
            });
            i += 1;
            continue;
        }
        // Number: [-+]?[0-9]+(\.[0-9]*)? | [-+]?\.[0-9]+  with optional [eE][-+]?[0-9]+
        if c == '+' || c == '-' || c == '.' || c.is_ascii_digit() {
            let start = i;
            if chars[i] == '+' || chars[i] == '-' {
                i += 1;
            }
            let mut has_digits = false;
            while i < n && chars[i].is_ascii_digit() {
                i += 1;
                has_digits = true;
            }
            if i < n && chars[i] == '.' {
                i += 1;
                while i < n && chars[i].is_ascii_digit() {
                    i += 1;
                    has_digits = true;
                }
            }
            if !has_digits {
                return None;
            }
            // Exponent
            if i < n && (chars[i] == 'e' || chars[i] == 'E') {
                let save = i;
                i += 1;
                if i < n && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                let mut exp_digits = false;
                while i < n && chars[i].is_ascii_digit() {
                    i += 1;
                    exp_digits = true;
                }
                if !exp_digits {
                    // Not actually an exponent; roll back.
                    i = save;
                }
            }
            let text: String = chars[start..i].iter().collect();
            tokens.push(Token {
                kind: TokKind::Number,
                text,
            });
            continue;
        }
        return None;
    }
    Some(tokens)
}

/// Port of `path-data-parser` `parsePath`.
pub fn parse_path(d: &str) -> Vec<Segment> {
    let tokens = match tokenize(d) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut segments: Vec<Segment> = Vec::new();
    let mut mode: char = '\0'; // '\0' == "BOD" (beginning of data)
    let mut index = 0usize;

    while index < tokens.len() {
        let token = &tokens[index];
        let params_count: usize;

        if mode == '\0' {
            let tt = token.text.chars().next().unwrap_or('\0');
            if tt == 'M' || tt == 'm' {
                index += 1;
                params_count = params_for(tt).unwrap();
                mode = tt;
            } else {
                // parsePath('M0,0' + d)
                let mut prefixed = String::from("M0,0");
                prefixed.push_str(d);
                return parse_path(&prefixed);
            }
        } else if token.kind == TokKind::Number {
            params_count = params_for(mode).unwrap();
        } else {
            let tt = token.text.chars().next().unwrap_or('\0');
            index += 1;
            params_count = match params_for(tt) {
                Some(p) => p,
                None => return Vec::new(),
            };
            mode = tt;
        }

        if index + params_count <= tokens.len() {
            let mut params = Vec::with_capacity(params_count);
            for t in tokens.iter().skip(index).take(params_count) {
                if t.kind == TokKind::Number {
                    params.push(t.text.parse::<f64>().unwrap_or(0.0));
                } else {
                    // "Param not a number" — bail out gracefully.
                    return segments;
                }
            }
            segments.push(Segment {
                key: mode,
                data: params,
            });
            index += params_count;
            if mode == 'M' {
                mode = 'L';
            } else if mode == 'm' {
                mode = 'l';
            }
        } else {
            // "Path data ended short" — bail out gracefully.
            break;
        }
    }
    segments
}

/// Port of `path-data-parser` `absolutize` — relative commands to absolute.
pub fn absolutize(segments: &[Segment]) -> Vec<Segment> {
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut subx = 0.0;
    let mut suby = 0.0;
    let mut out = Vec::new();
    for seg in segments {
        let d = &seg.data;
        match seg.key {
            'M' => {
                out.push(Segment { key: 'M', data: d.clone() });
                cx = d[0];
                cy = d[1];
                subx = d[0];
                suby = d[1];
            }
            'm' => {
                cx += d[0];
                cy += d[1];
                out.push(Segment { key: 'M', data: vec![cx, cy] });
                subx = cx;
                suby = cy;
            }
            'L' => {
                out.push(Segment { key: 'L', data: d.clone() });
                cx = d[0];
                cy = d[1];
            }
            'l' => {
                cx += d[0];
                cy += d[1];
                out.push(Segment { key: 'L', data: vec![cx, cy] });
            }
            'C' => {
                out.push(Segment { key: 'C', data: d.clone() });
                cx = d[4];
                cy = d[5];
            }
            'c' => {
                let nd: Vec<f64> = d
                    .iter()
                    .enumerate()
                    .map(|(i, v)| if i % 2 == 1 { v + cy } else { v + cx })
                    .collect();
                cx = nd[4];
                cy = nd[5];
                out.push(Segment { key: 'C', data: nd });
            }
            'Q' => {
                out.push(Segment { key: 'Q', data: d.clone() });
                cx = d[2];
                cy = d[3];
            }
            'q' => {
                let nd: Vec<f64> = d
                    .iter()
                    .enumerate()
                    .map(|(i, v)| if i % 2 == 1 { v + cy } else { v + cx })
                    .collect();
                cx = nd[2];
                cy = nd[3];
                out.push(Segment { key: 'Q', data: nd });
            }
            'A' => {
                out.push(Segment { key: 'A', data: d.clone() });
                cx = d[5];
                cy = d[6];
            }
            'a' => {
                cx += d[5];
                cy += d[6];
                out.push(Segment {
                    key: 'A',
                    data: vec![d[0], d[1], d[2], d[3], d[4], cx, cy],
                });
            }
            'H' => {
                out.push(Segment { key: 'H', data: d.clone() });
                cx = d[0];
            }
            'h' => {
                cx += d[0];
                out.push(Segment { key: 'H', data: vec![cx] });
            }
            'V' => {
                out.push(Segment { key: 'V', data: d.clone() });
                cy = d[0];
            }
            'v' => {
                cy += d[0];
                out.push(Segment { key: 'V', data: vec![cy] });
            }
            'S' => {
                out.push(Segment { key: 'S', data: d.clone() });
                cx = d[2];
                cy = d[3];
            }
            's' => {
                let nd: Vec<f64> = d
                    .iter()
                    .enumerate()
                    .map(|(i, v)| if i % 2 == 1 { v + cy } else { v + cx })
                    .collect();
                cx = nd[2];
                cy = nd[3];
                out.push(Segment { key: 'S', data: nd });
            }
            'T' => {
                out.push(Segment { key: 'T', data: d.clone() });
                cx = d[0];
                cy = d[1];
            }
            't' => {
                cx += d[0];
                cy += d[1];
                out.push(Segment { key: 'T', data: vec![cx, cy] });
            }
            'Z' | 'z' => {
                out.push(Segment { key: 'Z', data: vec![] });
                cx = subx;
                cy = suby;
            }
            _ => {}
        }
    }
    out
}

/// Port of `path-data-parser` `normalize` — reduce to only M/L/C/Z.
pub fn normalize(segments: &[Segment]) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut last_type = '\0';
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut subx = 0.0;
    let mut suby = 0.0;
    let mut lcx = 0.0;
    let mut lcy = 0.0;
    for seg in segments {
        let data = &seg.data;
        match seg.key {
            'M' => {
                out.push(Segment { key: 'M', data: data.clone() });
                cx = data[0];
                cy = data[1];
                subx = data[0];
                suby = data[1];
            }
            'C' => {
                out.push(Segment { key: 'C', data: data.clone() });
                cx = data[4];
                cy = data[5];
                lcx = data[2];
                lcy = data[3];
            }
            'L' => {
                out.push(Segment { key: 'L', data: data.clone() });
                cx = data[0];
                cy = data[1];
            }
            'H' => {
                cx = data[0];
                out.push(Segment { key: 'L', data: vec![cx, cy] });
            }
            'V' => {
                cy = data[0];
                out.push(Segment { key: 'L', data: vec![cx, cy] });
            }
            'S' => {
                let (cx1, cy1) = if last_type == 'C' || last_type == 'S' {
                    (cx + (cx - lcx), cy + (cy - lcy))
                } else {
                    (cx, cy)
                };
                out.push(Segment {
                    key: 'C',
                    data: vec![cx1, cy1, data[0], data[1], data[2], data[3]],
                });
                lcx = data[0];
                lcy = data[1];
                cx = data[2];
                cy = data[3];
            }
            'T' => {
                let x = data[0];
                let y = data[1];
                let (x1, y1) = if last_type == 'Q' || last_type == 'T' {
                    (cx + (cx - lcx), cy + (cy - lcy))
                } else {
                    (cx, cy)
                };
                let cx1 = cx + 2.0 * (x1 - cx) / 3.0;
                let cy1 = cy + 2.0 * (y1 - cy) / 3.0;
                let cx2 = x + 2.0 * (x1 - x) / 3.0;
                let cy2 = y + 2.0 * (y1 - y) / 3.0;
                out.push(Segment {
                    key: 'C',
                    data: vec![cx1, cy1, cx2, cy2, x, y],
                });
                lcx = x1;
                lcy = y1;
                cx = x;
                cy = y;
            }
            'Q' => {
                let x1 = data[0];
                let y1 = data[1];
                let x = data[2];
                let y = data[3];
                let cx1 = cx + 2.0 * (x1 - cx) / 3.0;
                let cy1 = cy + 2.0 * (y1 - cy) / 3.0;
                let cx2 = x + 2.0 * (x1 - x) / 3.0;
                let cy2 = y + 2.0 * (y1 - y) / 3.0;
                out.push(Segment {
                    key: 'C',
                    data: vec![cx1, cy1, cx2, cy2, x, y],
                });
                lcx = x1;
                lcy = y1;
                cx = x;
                cy = y;
            }
            'A' => {
                let r1 = data[0].abs();
                let r2 = data[1].abs();
                let angle = data[2];
                let large_arc_flag = data[3] != 0.0;
                let sweep_flag = data[4] != 0.0;
                let x = data[5];
                let y = data[6];
                if r1 == 0.0 || r2 == 0.0 {
                    out.push(Segment {
                        key: 'C',
                        data: vec![cx, cy, x, y, x, y],
                    });
                    cx = x;
                    cy = y;
                } else if cx != x || cy != y {
                    let curves = arc_to_cubic_curves(
                        cx,
                        cy,
                        x,
                        y,
                        r1,
                        r2,
                        angle,
                        large_arc_flag,
                        sweep_flag,
                        None,
                    );
                    for c in curves {
                        out.push(Segment { key: 'C', data: c });
                    }
                    cx = x;
                    cy = y;
                }
            }
            'Z' | 'z' => {
                out.push(Segment { key: 'Z', data: vec![] });
                cx = subx;
                cy = suby;
            }
            _ => {}
        }
        last_type = seg.key;
    }
    out
}

fn deg_to_rad(degrees: f64) -> f64 {
    (PI * degrees) / 180.0
}

fn rotate(x: f64, y: f64, angle_rad: f64) -> [f64; 2] {
    [
        x * angle_rad.cos() - y * angle_rad.sin(),
        x * angle_rad.sin() + y * angle_rad.cos(),
    ]
}

/// Port of `path-data-parser` `arcToCubicCurves`.
///
/// `recursive` carries `[f1, f2, cx, cy]` on recursive calls. Returns either raw
/// bezier control-point triples (recursive) or completed `C` data arrays.
#[allow(clippy::too_many_arguments)]
fn arc_to_cubic_curves(
    mut x1: f64,
    mut y1: f64,
    mut x2: f64,
    mut y2: f64,
    mut r1: f64,
    mut r2: f64,
    angle: f64,
    large_arc_flag: bool,
    sweep_flag: bool,
    recursive: Option<[f64; 4]>,
) -> Vec<Vec<f64>> {
    let angle_rad = deg_to_rad(angle);
    let mut params: Vec<[f64; 2]> = Vec::new();
    let (mut f1, mut f2, cx, cy);

    if let Some([rf1, rf2, rcx, rcy]) = recursive {
        f1 = rf1;
        f2 = rf2;
        cx = rcx;
        cy = rcy;
    } else {
        let p1 = rotate(x1, y1, -angle_rad);
        x1 = p1[0];
        y1 = p1[1];
        let p2 = rotate(x2, y2, -angle_rad);
        x2 = p2[0];
        y2 = p2[1];
        let x = (x1 - x2) / 2.0;
        let y = (y1 - y2) / 2.0;
        let mut h = (x * x) / (r1 * r1) + (y * y) / (r2 * r2);
        if h > 1.0 {
            h = h.sqrt();
            r1 *= h;
            r2 *= h;
        }
        let sign = if large_arc_flag == sweep_flag { -1.0 } else { 1.0 };
        let r1_pow = r1 * r1;
        let r2_pow = r2 * r2;
        let left = r1_pow * r2_pow - r1_pow * y * y - r2_pow * x * x;
        let right = r1_pow * y * y + r2_pow * x * x;
        let k = sign * (left / right).abs().sqrt();
        let ccx = k * r1 * y / r2 + (x1 + x2) / 2.0;
        let ccy = k * -r2 * x / r1 + (y1 + y2) / 2.0;
        cx = ccx;
        cy = ccy;
        // Math.asin(parseFloat(((y1 - cy) / r2).toFixed(9)))
        f1 = (round9((y1 - cy) / r2)).asin();
        f2 = (round9((y2 - cy) / r2)).asin();
        if x1 < cx {
            f1 = PI - f1;
        }
        if x2 < cx {
            f2 = PI - f2;
        }
        if f1 < 0.0 {
            f1 += PI * 2.0;
        }
        if f2 < 0.0 {
            f2 += PI * 2.0;
        }
        if sweep_flag && f1 > f2 {
            f1 -= PI * 2.0;
        }
        if !sweep_flag && f2 > f1 {
            f2 -= PI * 2.0;
        }
    }

    let mut df = f2 - f1;
    if df.abs() > (PI * 120.0 / 180.0) {
        let f2old = f2;
        let x2old = x2;
        let y2old = y2;
        if sweep_flag && f2 > f1 {
            f2 = f1 + (PI * 120.0 / 180.0);
        } else {
            f2 = f1 - (PI * 120.0 / 180.0);
        }
        x2 = cx + r1 * f2.cos();
        y2 = cy + r2 * f2.sin();
        params = arc_to_cubic_curves(
            x2,
            y2,
            x2old,
            y2old,
            r1,
            r2,
            angle,
            false,
            sweep_flag,
            Some([f2, f2old, cx, cy]),
        )
        .into_iter()
        .map(|v| [v[0], v[1]])
        .collect::<Vec<_>>();
    }

    df = f2 - f1;
    let c1 = f1.cos();
    let s1 = f1.sin();
    let c2 = f2.cos();
    let s2 = f2.sin();
    let t = (df / 4.0).tan();
    let hx = 4.0 / 3.0 * r1 * t;
    let hy = 4.0 / 3.0 * r2 * t;
    let m1 = [x1, y1];
    let mut m2 = [x1 + hx * s1, y1 - hy * c1];
    let m3 = [x2 + hx * s2, y2 - hy * c2];
    let m4 = [x2, y2];
    m2[0] = 2.0 * m1[0] - m2[0];
    m2[1] = 2.0 * m1[1] - m2[1];

    if recursive.is_some() {
        // Return the raw points (m2, m3, m4) followed by params.
        let mut out = vec![m2, m3, m4];
        out.extend(params);
        out.into_iter().map(|p| vec![p[0], p[1]]).collect()
    } else {
        let mut all = vec![m2, m3, m4];
        all.extend(params);
        let mut curves = Vec::new();
        let mut i = 0;
        while i < all.len() {
            let rr1 = rotate(all[i][0], all[i][1], angle_rad);
            let rr2 = rotate(all[i + 1][0], all[i + 1][1], angle_rad);
            let rr3 = rotate(all[i + 2][0], all[i + 2][1], angle_rad);
            curves.push(vec![rr1[0], rr1[1], rr2[0], rr2[1], rr3[0], rr3[1]]);
            i += 3;
        }
        curves
    }
}

/// JS `parseFloat(x.toFixed(9))`.
fn round9(x: f64) -> f64 {
    format!("{:.9}", x).parse::<f64>().unwrap_or(x)
}
