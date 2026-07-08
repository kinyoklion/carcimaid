//! `roughr` — a faithful, self-contained Rust port of the subset of
//! [rough.js](https://roughjs.com) 4.6.6 that [mermaid](https://mermaid.js.org)
//! uses to render its "hand-drawn" look.
//!
//! Derivative work of rough.js, (c) 2019 Preet Shihn, MIT licensed. See
//! `LICENSE-ROUGHJS` and `README.md`.
//!
//! # Determinism
//!
//! rough.js is non-deterministic when seeded with `0` because its PRNG routes
//! to `Math.random()` for a falsy seed. This crate's [`Generator`] defaults the
//! seed to a fixed **non-zero** value (`1`) so the Lehmer LCG always runs and
//! output is fully deterministic and stable.
//!
//! # Example
//!
//! ```
//! use roughr::Generator;
//!
//! let gen = Generator::new();
//! let o = gen.default_options();
//! let d = gen.rectangle(10.0, 10.0, 100.0, 60.0, &o);
//! // Two identical calls produce identical output.
//! let d2 = gen.rectangle(10.0, 10.0, 100.0, 60.0, &o);
//! assert_eq!(d.stroke_path(None), d2.stroke_path(None));
//! ```

pub mod core;
pub mod fillers;
pub mod generator;
pub mod math;
pub mod pathdata;
pub mod renderer;

pub use crate::core::{Op, OpSet, OpSetType, OpType, Options, Point};
pub use crate::generator::{ops_to_path, Drawable, Generator};
pub use crate::math::Random;

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Point> {
        vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]
    }

    #[test]
    fn polygon_is_deterministic() {
        let gen = Generator::new();
        let o = gen.default_options();
        let a = gen.polygon(&square(), &o);
        let b = gen.polygon(&square(), &o);
        assert_eq!(a.stroke_path(None), b.stroke_path(None));
        assert!(!a.stroke_path(None).is_empty());
    }

    #[test]
    fn circle_is_deterministic() {
        let gen = Generator::new();
        let o = gen.default_options();
        let a = gen.circle(50.0, 50.0, 80.0, &o);
        let b = gen.circle(50.0, 50.0, 80.0, &o);
        assert_eq!(a.stroke_path(None), b.stroke_path(None));
        assert!(!a.stroke_path(None).is_empty());
    }

    #[test]
    fn path_is_deterministic() {
        let gen = Generator::new();
        let o = gen.default_options();
        let d = "M0 0 L100 0 L100 100 Z";
        let a = gen.path(d, &o);
        let b = gen.path(d, &o);
        assert_eq!(a.stroke_path(None), b.stroke_path(None));
        assert!(!a.stroke_path(None).is_empty());
    }

    #[test]
    fn different_seeds_differ() {
        let gen = Generator::new();
        let mut o1 = gen.default_options();
        o1.seed = 1;
        let mut o2 = gen.default_options();
        o2.seed = 42;
        let a = gen.polygon(&square(), &o1);
        let b = gen.polygon(&square(), &o2);
        assert_ne!(a.stroke_path(None), b.stroke_path(None));
    }

    #[test]
    fn roughness_zero_is_deterministic_and_smooth() {
        let gen = Generator::new();
        let mut o = gen.default_options();
        o.roughness = 0.0;
        let a = gen.polygon(&square(), &o);
        let b = gen.polygon(&square(), &o);
        assert_eq!(a.stroke_path(None), b.stroke_path(None));
    }

    #[test]
    fn hachure_fill_is_deterministic_and_nonempty() {
        let gen = Generator::new();
        let mut o = gen.default_options();
        o.fill = Some("#f00".to_string()); // fillStyle defaults to "hachure"
        let a = gen.rectangle(0.0, 0.0, 100.0, 100.0, &o);
        let b = gen.rectangle(0.0, 0.0, 100.0, 100.0, &o);
        assert_eq!(a.fill_path(None), b.fill_path(None));
        assert!(
            !a.fill_path(None).is_empty(),
            "hachure fill should emit ops"
        );
        assert!(!a.stroke_path(None).is_empty());
    }

    #[test]
    fn solid_fill_is_deterministic() {
        let gen = Generator::new();
        let mut o = gen.default_options();
        o.fill = Some("#00f".to_string());
        o.fill_style = "solid".to_string();
        let a = gen.circle(50.0, 50.0, 80.0, &o);
        let b = gen.circle(50.0, 50.0, 80.0, &o);
        assert_eq!(a.fill_path(None), b.fill_path(None));
        assert!(!a.fill_path(None).is_empty());
    }

    // --- Pinned regression anchors (values produced by this port; see PIN_* ) ---

    #[test]
    fn pinned_polygon_roughness_0_7() {
        let gen = Generator::new();
        let mut o = gen.default_options();
        o.roughness = 0.7;
        let d = gen.polygon(&square(), &o);
        assert_eq!(d.stroke_path(None), PIN_POLYGON_R07);
    }

    #[test]
    fn pinned_rectangle_roughness_0() {
        let gen = Generator::new();
        let mut o = gen.default_options();
        o.roughness = 0.0;
        let d = gen.rectangle(10.0, 10.0, 100.0, 60.0, &o);
        assert_eq!(d.stroke_path(None), PIN_RECT_R0);
    }

    #[test]
    fn pinned_path_fixed_decimals() {
        let gen = Generator::new();
        let mut o = gen.default_options();
        o.roughness = 0.7;
        let d = gen.path("M0 0 L100 0 L100 100 Z", &o);
        // Fixed-decimal serialization is stable too.
        assert_eq!(d.stroke_path(Some(2)), PIN_PATH_R07_FIXED2);
    }

    // Pinned values (regenerate intentionally if the algorithm changes).
    const PIN_POLYGON_R07: &str = "M0.6000844134017824 0.6747193174436688 C19.77662188205868 0.6723603793419898, 39.60881254132837 -1.352974986191839, 99.4444837404415 0.2746348517015576 M-0.45406589424237603 0.18521902626380324 C22.442805964965377 0.2673885881900787, 44.330852998886265 -0.13138224184513092, 99.59373799944296 0.5269711113534867 M101.07569297086448 -1.2246034009382127 C100.44602806763723 22.611411325447264, 100.9301031907089 42.35190774891526, 100.9733834279701 99.39145154524594 M100.20837439680471 -0.5594918393529951 C100.38977821636945 26.23749319119379, 99.70625024307519 53.698269782867285, 100.28481379235163 100.24657060569152 M100.73696112763136 99.85059189368039 C79.15405219662935 100.55736135495827, 55.68698932733387 100.176894077193, 1.2855196190997957 99.71753356624394 M99.339601404313 99.89938759272918 C62.34723033355549 100.2647945297882, 25.37462940821424 100.07476946543903, 0.6572648026980459 100.02929103737696 M-0.504422883875668 99.80297243762762 C0.3745224661193788 70.24920164253562, -0.8381808989681303 42.764133735932404, 0.4142602337524295 -0.844256536476314 M0.1525698401965201 100.69875612622127 C-0.9736857950687408 63.16561618773267, -0.6194476932287216 26.61640330841765, 0.30851839864626524 0.6916210538707673";
    const PIN_RECT_R0: &str = "M10 10 C30.000449558719993 10, 50.000899117439985 10, 110 10 M10 10 C32.135189184919 10, 54.27037836983801 10, 110 10 M110 10 C110 23.05013837851584, 110 36.10027675703168, 110 70 M110 10 C110 26.103745287284255, 110 42.20749057456851, 110 70 M110 70 C88.43275235034525 70, 66.86550470069051 70, 10 70 M110 70 C72.40874170325696 70, 34.81748340651393 70, 10 70 M10 70 C10 52.79144093953073, 10 35.58288187906145, 10 10 M10 70 C10 48.17402392067015, 10 26.348047841340303, 10 10";
    const PIN_PATH_R07_FIXED2: &str = "M0.6 0.67 C19.78 0.67, 39.61 -1.35, 99.44 0.27 M-0.45 0.19 C22.44 0.27, 44.33 -0.13, 99.59 0.53 M101.08 -1.22 C100.45 22.61, 100.93 42.35, 100.97 99.39 M100.21 -0.56 C100.39 26.24, 99.71 53.7, 100.28 100.25 M100.74 99.85 C78.97 78.99, 55.5 57.04, 1.29 -0.28 M99.34 99.9 C62.51 62.67, 25.54 24.89, 0.66 0.03";
}
