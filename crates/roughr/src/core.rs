//! Port of rough.js `core.js` types: `OpType`, `Op`, `OpSet`/`OpSetType`, and
//! the resolved options (`ResolvedOptions`).
//!
//! rough.js keeps these as TypeScript interfaces (erased in the shipped `.js`);
//! we model them as concrete Rust types.

use crate::math::Random;

/// A 2D point `[x, y]`.
pub type Point = [f64; 2];

/// rough.js `OpType`: the drawing operation of a single `Op`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpType {
    /// `move` — moveTo.
    Move,
    /// `bcurveTo` — cubic bezier curveTo.
    BCurveTo,
    /// `lineTo`.
    LineTo,
}

/// rough.js `Op` — one operation with its numeric data.
#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub op: OpType,
    pub data: Vec<f64>,
}

impl Op {
    pub fn new(op: OpType, data: Vec<f64>) -> Self {
        Op { op, data }
    }
}

/// rough.js `OpSetType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpSetType {
    /// `path` — a stroke path.
    Path,
    /// `fillPath` — a solid fill path.
    FillPath,
    /// `fillSketch` — a sketchy (e.g. hachure) fill.
    FillSketch,
}

/// rough.js `OpSet` — a set of ops of a given type.
#[derive(Clone, Debug)]
pub struct OpSet {
    pub op_set_type: OpSetType,
    pub ops: Vec<Op>,
}

impl OpSet {
    pub fn new(op_set_type: OpSetType, ops: Vec<Op>) -> Self {
        OpSet { op_set_type, ops }
    }
}

/// rough.js resolved options (`ResolvedOptions`).
///
/// `Default` mirrors rough.js's `RoughGenerator.defaultOptions` **except** that
/// `seed` is `0` here (matching rough.js `ResolvedOptions` upstream). The
/// [`crate::generator::Generator`] overrides `seed` to `1` so that all generated
/// output is deterministic. Constructing `Options::default()` directly and using
/// `seed == 0` yields a degenerate (all-zero-offset) but still deterministic
/// result — prefer [`crate::generator::Generator::default_options`].
#[derive(Clone, Debug)]
pub struct Options {
    pub max_randomness_offset: f64,
    pub roughness: f64,
    pub bowing: f64,
    pub stroke: String,
    pub stroke_width: f64,
    pub curve_tightness: f64,
    pub curve_fitting: f64,
    pub curve_step_count: f64,
    /// Fill color. `None` means "no fill" (rough.js `undefined`).
    pub fill: Option<String>,
    pub fill_style: String,
    pub fill_weight: f64,
    pub hachure_angle: f64,
    pub hachure_gap: f64,
    pub dash_offset: f64,
    pub dash_gap: f64,
    pub zigzag_offset: f64,
    pub seed: i32,
    pub disable_multi_stroke: bool,
    pub disable_multi_stroke_fill: bool,
    pub preserve_vertices: bool,
    pub fill_shape_roughness_gain: f64,

    /// Internal, cached PRNG. Mirrors rough.js `Options.randomizer`, which is
    /// created lazily and threaded through a single shape's generation so all
    /// offsets share one advancing sequence. Reset to `None` at the start of
    /// each `Generator` method call.
    pub(crate) randomizer: Option<Random>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            max_randomness_offset: 2.0,
            roughness: 1.0,
            bowing: 1.0,
            stroke: "#000".to_string(),
            stroke_width: 1.0,
            curve_tightness: 0.0,
            curve_fitting: 0.95,
            curve_step_count: 9.0,
            fill: None,
            fill_style: "hachure".to_string(),
            fill_weight: -1.0,
            hachure_angle: -41.0,
            hachure_gap: -1.0,
            dash_offset: -1.0,
            dash_gap: -1.0,
            zigzag_offset: -1.0,
            seed: 0,
            disable_multi_stroke: false,
            disable_multi_stroke_fill: false,
            preserve_vertices: false,
            fill_shape_roughness_gain: 0.8,
            randomizer: None,
        }
    }
}

impl Options {
    /// The "no stroke" / "no fill" sentinel, rough.js `'none'`.
    pub const NONE: &'static str = "none";
}
