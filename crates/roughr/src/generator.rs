//! Port of rough.js `generator.js`: the public `RoughGenerator` API, its default
//! options, `Drawable`/`OpSet` assembly, and `opsToPath` serialization.
//!
//! Deviation from rough.js: [`Generator::new`] sets the default `seed` to `1`
//! (non-zero) so output is fully deterministic. rough.js defaults `seed` to `0`,
//! which routes its PRNG to the non-deterministic `Math.random()`.

use crate::core::{Op, OpSet, OpSetType, OpType, Options, Point};
use crate::pathdata::{absolutize, normalize, parse_path};
use crate::renderer::{self, ellipse_with_params, generate_ellipse_params, solid_fill_polygon};

/// rough.js `Drawable`.
#[derive(Clone, Debug)]
pub struct Drawable {
    pub shape: String,
    pub options: Options,
    pub sets: Vec<OpSet>,
}

impl Drawable {
    /// Serialize all stroke op-sets (`OpSetType::Path`) to a single SVG path
    /// `d` string. `fixed_decimals` mirrors rough.js `opsToPath`'s optional
    /// decimal rounding (`None` = full precision, as rough.js `toPaths` uses).
    pub fn stroke_path(&self, fixed_decimals: Option<usize>) -> String {
        self.combined(OpSetType::Path, fixed_decimals)
    }

    /// Serialize all fill op-sets (`OpSetType::FillPath` or `FillSketch`) to a
    /// single SVG path `d` string.
    pub fn fill_path(&self, fixed_decimals: Option<usize>) -> String {
        let mut parts = Vec::new();
        for set in &self.sets {
            if matches!(set.op_set_type, OpSetType::FillPath | OpSetType::FillSketch) {
                let s = ops_to_path(set, fixed_decimals);
                if !s.is_empty() {
                    parts.push(s);
                }
            }
        }
        parts.join(" ")
    }

    fn combined(&self, t: OpSetType, fixed_decimals: Option<usize>) -> String {
        let mut parts = Vec::new();
        for set in &self.sets {
            if set.op_set_type == t {
                let s = ops_to_path(set, fixed_decimals);
                if !s.is_empty() {
                    parts.push(s);
                }
            }
        }
        parts.join(" ")
    }
}

/// The rough.js hand-drawn shape generator.
#[derive(Clone, Debug)]
pub struct Generator {
    default_options: Options,
}

impl Default for Generator {
    fn default() -> Self {
        Self::new()
    }
}

impl Generator {
    /// Create a generator with rough.js's default options, except `seed = 1`
    /// (deterministic).
    pub fn new() -> Self {
        // seed = 1 (non-zero): deliberate deviation from rough.js for determinism.
        let default_options = Options {
            seed: 1,
            ..Options::default()
        };
        Generator { default_options }
    }

    /// A clone of this generator's default options (with `seed = 1`). Tweak the
    /// returned value and pass it to the drawing methods.
    pub fn default_options(&self) -> Options {
        self.default_options.clone()
    }

    fn prep(options: &Options) -> Options {
        let mut o = options.clone();
        o.randomizer = None; // fresh PRNG per shape, like rough.js `_o`
        o
    }

    fn d(shape: &str, sets: Vec<OpSet>, options: Options) -> Drawable {
        Drawable {
            shape: shape.to_string(),
            sets,
            options,
        }
    }

    pub fn line(&self, x1: f64, y1: f64, x2: f64, y2: f64, options: &Options) -> Drawable {
        let mut o = Self::prep(options);
        let set = renderer::line(x1, y1, x2, y2, &mut o);
        Self::d("line", vec![set], o)
    }

    pub fn rectangle(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        options: &Options,
    ) -> Drawable {
        let mut o = Self::prep(options);
        let outline = renderer::rectangle(x, y, width, height, &mut o);
        let mut paths = Vec::new();
        if o.fill.is_some() {
            let points = vec![
                [x, y],
                [x + width, y],
                [x + width, y + height],
                [x, y + height],
            ];
            if o.fill_style == "solid" {
                paths.push(solid_fill_polygon(&[points], &mut o));
            } else {
                paths.push(renderer::pattern_fill_polygons(&[points], &mut o));
            }
        }
        if o.stroke != Options::NONE {
            paths.push(outline);
        }
        Self::d("rectangle", paths, o)
    }

    pub fn ellipse(&self, x: f64, y: f64, width: f64, height: f64, options: &Options) -> Drawable {
        let mut o = Self::prep(options);
        let params = generate_ellipse_params(width, height, &mut o);
        let response = ellipse_with_params(x, y, &mut o, &params);
        let mut paths = Vec::new();
        if o.fill.is_some() {
            if o.fill_style == "solid" {
                let mut shape = ellipse_with_params(x, y, &mut o, &params).opset;
                shape.op_set_type = OpSetType::FillPath;
                paths.push(shape);
            } else {
                paths.push(renderer::pattern_fill_polygons(
                    std::slice::from_ref(&response.estimated_points),
                    &mut o,
                ));
            }
        }
        if o.stroke != Options::NONE {
            paths.push(response.opset);
        }
        Self::d("ellipse", paths, o)
    }

    pub fn circle(&self, x: f64, y: f64, diameter: f64, options: &Options) -> Drawable {
        let mut ret = self.ellipse(x, y, diameter, diameter, options);
        ret.shape = "circle".to_string();
        ret
    }

    pub fn linear_path(&self, points: &[Point], options: &Options) -> Drawable {
        let mut o = Self::prep(options);
        let set = renderer::linear_path(points, false, &mut o);
        Self::d("linearPath", vec![set], o)
    }

    pub fn polygon(&self, points: &[Point], options: &Options) -> Drawable {
        let mut o = Self::prep(options);
        let outline = renderer::linear_path(points, true, &mut o);
        let mut paths = Vec::new();
        if o.fill.is_some() {
            if o.fill_style == "solid" {
                paths.push(solid_fill_polygon(&[points.to_vec()], &mut o));
            } else {
                paths.push(renderer::pattern_fill_polygons(&[points.to_vec()], &mut o));
            }
        }
        if o.stroke != Options::NONE {
            paths.push(outline);
        }
        Self::d("polygon", paths, o)
    }

    /// rough.js `arc`. `closed`/fill supported (solid + hachure).
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        start: f64,
        stop: f64,
        closed: bool,
        options: &Options,
    ) -> Drawable {
        let mut o = Self::prep(options);
        let outline = renderer::arc(x, y, width, height, start, stop, closed, true, &mut o);
        let mut paths = Vec::new();
        if closed && o.fill.is_some() {
            if o.fill_style == "solid" {
                let mut fo = o.clone();
                fo.disable_multi_stroke = true;
                let mut shape =
                    renderer::arc(x, y, width, height, start, stop, true, false, &mut fo);
                shape.op_set_type = OpSetType::FillPath;
                paths.push(shape);
                // Keep the shared PRNG state advanced consistently.
                o.randomizer = fo.randomizer;
            } else {
                paths.push(renderer::pattern_fill_arc(
                    x, y, width, height, start, stop, &mut o,
                ));
            }
        }
        if o.stroke != Options::NONE {
            paths.push(outline);
        }
        Self::d("arc", paths, o)
    }

    /// rough.js `curve`. Stroke + solid fill supported. (Hachure fill on curves
    /// is omitted; it requires the `points-on-curve` bezier sampler.)
    pub fn curve(&self, points: &[Point], options: &Options) -> Drawable {
        let mut o = Self::prep(options);
        let outline = renderer::curve(points, &mut o);
        let mut paths = Vec::new();
        if o.fill.is_some() && o.fill.as_deref() != Some(Options::NONE) && o.fill_style == "solid" {
            let mut fo = o.clone();
            fo.disable_multi_stroke = true;
            fo.roughness = if o.roughness != 0.0 {
                o.roughness + o.fill_shape_roughness_gain
            } else {
                0.0
            };
            let fill_shape = renderer::curve(points, &mut fo);
            o.randomizer = fo.randomizer;
            paths.push(OpSet::new(
                OpSetType::FillPath,
                merged_shape(fill_shape.ops),
            ));
        }
        if o.stroke != Options::NONE {
            paths.push(outline);
        }
        Self::d("curve", paths, o)
    }

    /// rough.js `path`. Stroke via `svgPath`; solid fill via the merged
    /// single-subpath shape; hachure fill via a flattened polyline of the path.
    ///
    /// Simplifications vs rough.js: the `simplification` option and the
    /// `points-on-path` distance-tolerant sampler are not implemented. Fill
    /// polylines are produced by fixed cubic-bezier subdivision.
    pub fn path(&self, d: &str, options: &Options) -> Drawable {
        let mut o = Self::prep(options);
        let mut paths = Vec::new();
        if d.is_empty() {
            return Self::d("path", paths, o);
        }
        // rough.js: d.replace(/\n/g,' ').replace(/(-\s)/g,'-') (the third
        // replace in rough.js is a no-op due to a quoting bug).
        let d_clean = d.replace('\n', " ").replace("- ", "-");

        let has_fill = match &o.fill {
            Some(f) => f != "transparent" && f != Options::NONE,
            None => false,
        };
        let has_stroke = o.stroke != Options::NONE;

        let sets = flatten_path(&d_clean);
        let shape = renderer::svg_path(&d_clean, &mut o);

        if has_fill {
            if o.fill_style == "solid" {
                if sets.len() == 1 {
                    let mut fo = o.clone();
                    fo.disable_multi_stroke = true;
                    fo.roughness = if o.roughness != 0.0 {
                        o.roughness + o.fill_shape_roughness_gain
                    } else {
                        0.0
                    };
                    let fill_shape = renderer::svg_path(&d_clean, &mut fo);
                    o.randomizer = fo.randomizer;
                    paths.push(OpSet::new(
                        OpSetType::FillPath,
                        merged_shape(fill_shape.ops),
                    ));
                } else {
                    paths.push(solid_fill_polygon(&sets, &mut o));
                }
            } else {
                paths.push(renderer::pattern_fill_polygons(&sets, &mut o));
            }
        }

        if has_stroke {
            paths.push(shape);
        }

        Self::d("path", paths, o)
    }
}

/// rough.js `_mergedShape`: keep op 0, drop any subsequent `move` ops.
fn merged_shape(input: Vec<Op>) -> Vec<Op> {
    input
        .into_iter()
        .enumerate()
        .filter(|(i, op)| *i == 0 || op.op != OpType::Move)
        .map(|(_, op)| op)
        .collect()
}

/// Flatten an SVG path `d` into polylines (one per subpath), sampling cubic
/// beziers with a fixed subdivision. Used only to feed the fill routines.
fn flatten_path(d: &str) -> Vec<Vec<Point>> {
    const STEPS: usize = 16;
    let segs = normalize(&absolutize(&parse_path(d)));
    let mut sets: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    let mut current: Point = [0.0, 0.0];
    let mut first: Point = [0.0, 0.0];
    for seg in &segs {
        let dd = &seg.data;
        match seg.key {
            'M' => {
                if !cur.is_empty() {
                    sets.push(std::mem::take(&mut cur));
                }
                current = [dd[0], dd[1]];
                first = current;
                cur.push(current);
            }
            'L' => {
                current = [dd[0], dd[1]];
                cur.push(current);
            }
            'C' => {
                let p0 = current;
                let c1 = [dd[0], dd[1]];
                let c2 = [dd[2], dd[3]];
                let p3 = [dd[4], dd[5]];
                for s in 1..=STEPS {
                    let t = s as f64 / STEPS as f64;
                    cur.push(cubic_point(p0, c1, c2, p3, t));
                }
                current = p3;
            }
            'Z' => {
                cur.push(first);
                current = first;
            }
            _ => {}
        }
    }
    if !cur.is_empty() {
        sets.push(cur);
    }
    sets
}

fn cubic_point(p0: Point, c1: Point, c2: Point, p3: Point, t: f64) -> Point {
    let mt = 1.0 - t;
    let a = mt * mt * mt;
    let b = 3.0 * mt * mt * t;
    let c = 3.0 * mt * t * t;
    let dd = t * t * t;
    [
        a * p0[0] + b * c1[0] + c * c2[0] + dd * p3[0],
        a * p0[1] + b * c1[1] + c * c2[1] + dd * p3[1],
    ]
}

/// rough.js `opsToPath`. `fixed_decimals` = `Some(n)` rounds each number to `n`
/// decimals (like rough.js when passed `fixedDecimals`); `None` uses full
/// precision (as rough.js `toPaths` does).
pub fn ops_to_path(op_set: &OpSet, fixed_decimals: Option<usize>) -> String {
    let mut path = String::new();
    for item in &op_set.ops {
        let d: Vec<f64> = match fixed_decimals {
            Some(n) => item.data.iter().map(|v| round_fixed(*v, n)).collect(),
            None => item.data.clone(),
        };
        match item.op {
            OpType::Move => {
                path.push_str(&format!("M{} {} ", fmt_num(d[0]), fmt_num(d[1])));
            }
            OpType::BCurveTo => {
                path.push_str(&format!(
                    "C{} {}, {} {}, {} {} ",
                    fmt_num(d[0]),
                    fmt_num(d[1]),
                    fmt_num(d[2]),
                    fmt_num(d[3]),
                    fmt_num(d[4]),
                    fmt_num(d[5]),
                ));
            }
            OpType::LineTo => {
                path.push_str(&format!("L{} {} ", fmt_num(d[0]), fmt_num(d[1])));
            }
        }
    }
    path.trim().to_string()
}

/// JS `+d.toFixed(n)` — round to `n` decimals then re-parse (drops trailing 0s).
fn round_fixed(d: f64, n: usize) -> f64 {
    format!("{:.*}", n, d).parse::<f64>().unwrap_or(d)
}

/// Format a number the way JS `${number}` (Number.prototype.toString) does for
/// the coordinate ranges rough.js emits: shortest round-trip, no trailing
/// `.0`, and `-0` normalized to `0`.
fn fmt_num(d: f64) -> String {
    let d = if d == 0.0 { 0.0 } else { d }; // normalize -0.0 -> 0
    format!("{}", d)
}
