# roughr

`roughr` is a **partial Rust port / derivative** of
[rough.js](https://roughjs.com) **4.6.6** — specifically the subset that
[mermaid](https://mermaid.js.org) uses to render its "hand-drawn" look.

rough.js is © 2019 Preet Shihn and is MIT licensed. This crate is likewise MIT
licensed. A verbatim copy of the rough.js license is included as
[`LICENSE-ROUGHJS`](./LICENSE-ROUGHJS).

This crate is pure Rust with **no external dependencies**.

## Deliberate deviation: deterministic by default

rough.js is **non-deterministic** when seeded with `0`, because its PRNG does:

```js
next() { if (this.seed) { /* LCG */ } else { return Math.random(); } }
```

`0` is falsy, so a `0` seed routes to `Math.random()`.

`roughr` avoids this: the [`Generator`] defaults `seed` to a fixed **non-zero**
value (`1`), so the Lehmer / Park–Miller LCG always runs and output is fully
deterministic and stable across runs and platforms (modulo libm differences in
transcendental functions — see below). We are **not** attempting to byte-match
any mermaid/rough.js render; the goal is a correct, deterministic renderer.

The PRNG itself (`Random::next`) is a faithful bit-for-bit port using 32-bit
wrapping integer math (`i32::wrapping_mul`, matching JS `Math.imul`). With
`seed = 0` it produces a degenerate all-zero sequence (rather than
`Math.random()`), which remains deterministic.

## What was ported

- **`math`** — the `Random` PRNG (`Random::next`) and `randomSeed` (stubbed to a
  fixed seed since no RNG dependency is shipped).
- **`core`** — `OpType` / `Op` / `OpSet` / `OpSetType` and the resolved
  `Options` (with rough.js defaults: `maxRandomnessOffset 2`, `roughness 1`,
  `bowing 1`, `curveFitting 0.95`, `curveTightness 0`, `curveStepCount 9`,
  `fillStyle "hachure"`, `fillWeight -1`, `hachureAngle -41`, `hachureGap -1`,
  `dashOffset/dashGap/zigzagOffset -1`, `disableMultiStroke(Fill) false`,
  `preserveVertices false`, `fillShapeRoughnessGain 0.8`; `seed 0` in the struct
  default, overridden to `1` by the `Generator`).
- **`renderer`** — `_line`, `_doubleLine`, `line`, `linearPath`, `polygon`,
  `rectangle`, `_curve` / `_curveWithOffset` / `curve`, `generateEllipseParams`,
  `ellipseWithParams`, `_computeEllipsePoints`, `ellipse`, `_arc` / `arc`,
  `_bezierTo`, `svgPath`, `solidFillPolygon`, `patternFillPolygons`,
  `patternFillArc`.
- **`pathdata`** — the `path-data-parser` package (`parsePath`, `absolutize`,
  `normalize`, including `arcToCubicCurves`) that `svgPath` depends on.
- **`fillers`** — the `hachure` fill path only: `polygonHachureLines`,
  `HachureFiller`, and the `hachure-fill` package's scan-line `hachureLines`.
- **`generator`** — the public `Generator` (`RoughGenerator`): `line`,
  `rectangle`, `ellipse`, `circle`, `linear_path`, `polygon`, `arc`, `curve`,
  `path`, plus `opsToPath` serialization and `Drawable`.

## Public API

```rust
use roughr::Generator;

let gen = Generator::new();          // default seed = 1 (deterministic)
let mut o = gen.default_options();   // tweak fields as needed
o.roughness = 0.7;

let d = gen.rectangle(10.0, 10.0, 100.0, 60.0, &o);
let stroke_d: String = d.stroke_path(None);       // OpSetType::Path -> SVG "d"
let fill_d:   String = d.fill_path(None);          // FillPath / FillSketch -> "d"
```

`Generator` methods: `line`, `rectangle`, `ellipse`, `circle`, `linear_path`,
`polygon`, `path`, `curve`, `arc` — each returns a
`Drawable { shape, options, sets: Vec<OpSet> }`.

Serialization helpers (`Drawable::stroke_path` / `fill_path`, and the free
`ops_to_path`) replicate rough.js `opsToPath` formatting: `M x y `,
`C x1 y1, x2 y2, x y `, `L x y `. Passing `None` uses full precision (as
rough.js `toPaths`); `Some(n)` rounds each coordinate to `n` decimals.

## Simplifications / omissions

- **Fill styles**: only `solid` and `hachure` are implemented (mermaid uses only
  these). `zigzag`, `cross-hatch`, `dots`, `dashed`, `zigzag-line` are omitted.
- **`path` fills**: rough.js uses `points-on-path` (a distance-tolerant bezier
  sampler) and a `simplification` option to build fill polylines. `roughr`
  instead flattens the path with fixed cubic-bezier subdivision and does not
  implement `simplification`. Path *stroke* output (`svgPath`) and *solid* fill
  (the merged single-subpath shape) are faithful.
- **`curve` fills**: hachure fill on `curve` is omitted (it needs the
  `points-on-curve` sampler); stroke and solid fill are supported.
- **Number formatting**: `opsToPath` uses Rust's shortest-round-trip float
  formatting, which matches JS `Number.toString` for the coordinate ranges
  rough.js emits (it does not reproduce JS exponential notation for extreme
  magnitudes, which do not occur here).
- Transcendental functions (`sin`/`cos`/`sqrt`/`tan`) may differ from V8 in the
  last ULP; output is self-consistent and deterministic but is not intended to
  byte-match a JavaScript render.

## License

MIT. Derivative of rough.js © 2019 Preet Shihn (MIT). See `LICENSE-ROUGHJS`.
