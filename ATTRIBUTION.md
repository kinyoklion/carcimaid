# Attribution

carcimaid is developed for behavioural compliance with [mermaid][mermaid] and
derives from several open-source projects. This file records every external
work we reference, reuse, or compare against, and the license under which we do
so. carcimaid itself is MIT-licensed (see `LICENSE`).

## Reference & comparison

### mermaid — MIT
- Repository: <https://github.com/mermaid-js/mermaid>
- License: MIT — `Copyright (c) 2014 - 2022 Knut Sveidqvist` (full text below).
- How we use it:
  - **Oracle**: the official mermaid CLI (`@mermaid-js/mermaid-cli`, run via the
    `minlag/mermaid-cli` Docker image) renders the reference SVG that our
    compliance harness diffs against.
  - **Grammar reference**: our parsers follow the syntax defined in mermaid's
    grammars and feature tests — flowchart
    (`packages/mermaid/src/diagrams/flowchart/parser/flow.jison` and its
    `*.spec.js` tests) and sequence
    (`packages/mermaid/src/diagrams/sequence/parser/sequenceDiagram.jison`).
  - **SVG structure reference / ported layout**: our renderer targets the DOM
    structure produced by mermaid's renderers (flowchart:
    `flowRenderer-v3-unified.ts`, `rendering-elements/nodes.ts`, `edges.js`,
    `markers.js`; sequence: `sequenceRenderer.ts`, `svgDraw.js`). The sequence
    layout (`crates/carcimaid/src/layout/sequence.rs`) is a Rust port of the
    geometry in mermaid's `sequenceRenderer`.
  - **Verbatim fragments**: the 12 flowchart arrowhead `<marker>` definitions
    (`crates/carcimaid/src/render/markers.rs`) and the sequence actor-icon
    `<symbol>`/marker `<defs>` (`crates/carcimaid/src/render/seq_defs.rs`) are
    reproduced verbatim from mermaid's output so the structural comparison
    matches element-for-element.
  - **Corpus**: diagram source text harvested from mermaid's `cypress/` specs
    and `demos/` is vendored under `corpus/` with attribution — see
    `corpus/ATTRIBUTION.md`.

Because verbatim mermaid-derived material is redistributed in this repository
(corpus text, marker/symbol defs), mermaid's full MIT notice is reproduced
here:

```text
MIT License

Copyright (c) 2014 - 2022 Knut Sveidqvist

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### dagre (dagrejs) — MIT
- Repository: <https://github.com/dagrejs/dagre>
- License: MIT
- The layered graph-layout algorithm mermaid uses for flowcharts. Our layout
  engine reimplements / ports this algorithm (rank → order → coordinate
  assignment).

### dagre (kookyleo/dagre-rs) — Apache-2.0 — **dependency**
- Crate: `dagre` <https://github.com/kookyleo/dagre-rs>
- A faithful Rust port of dagre.js. carcimaid's flowchart layout
  (`crates/carcimaid/src/layout.rs`) drives this crate with mermaid's
  parameters; it reproduces mermaid's node coordinates, edge waypoints, and
  diagram dimensions. Apache-2.0 is compatible with this project's MIT license;
  note that redistributing compiled binaries that link this crate requires
  shipping a copy of the Apache-2.0 license alongside them.
- Alternatives evaluated: `rust-sugiyama` (MIT) as a fallback core;
  `layout-rs` (MIT) for SVG-emission patterns.

### rough.js — MIT — **ported (derivative work)**
- Repository: <https://github.com/rough-stuff/rough> — © 2019 Preet Shihn, MIT.
- `crates/roughr` is a partial Rust port of rough.js 4.6.6 (the subset mermaid
  uses for its hand-drawn `look`), including the bundled `path-data-parser` and
  `hachure-fill` packages by the same author (also MIT). A verbatim copy of the
  rough.js license is vendored at `crates/roughr/LICENSE-ROUGHJS`; see
  `crates/roughr/README.md` for exactly what was ported and where we
  deliberately deviate (deterministic seeding).
- Note: an unrelated crates.io crate named `roughr` (a different rough.js port)
  already exists; ours is a path dependency and is not published.

### d3 (d3-shape / d3-path) — ISC
- Repository: <https://github.com/d3/d3-shape>, <https://github.com/d3/d3-path>
- License: ISC — `Copyright 2010-2023 Mike Bostock`
- The flowchart edge curve is generated by reimplementing d3's `curveBasis`
  open B-spline (mermaid's default flowchart edge curve) in
  `crates/carcimaid/src/render.rs` (`curve_basis`), and path coordinates are
  formatted the way `d3-path` formats them. ISC notice:

```text
Copyright 2010-2023 Mike Bostock

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.
```

### DejaVu Sans — Bitstream Vera / public-domain (permissive)
- Upstream: <https://dejavu-fonts.github.io/>
- License: Bitstream Vera Fonts Copyright (permissive); DejaVu changes are
  public domain. Full text vendored at
  `crates/carcimaid/resources/DejaVuSans-LICENSE.txt`.
- Why we ship it: the mermaid CLI's headless Chromium resolves mermaid's default
  font stack (`"trebuchet ms", verdana, arial, sans-serif`) to **DejaVu Sans**.
  carcimaid measures label text with the very same font file
  (`crates/carcimaid/resources/DejaVuSans.ttf`) so node/label sizes match
  mermaid's. See `crates/carcimaid/src/text.rs`.

### warpdotdev/mermaid-to-svg — MIT
- Repository: <https://github.com/warpdotdev/mermaid-to-svg>
- An existing pure-Rust mermaid→SVG implementation studied as an end-to-end
  architectural reference.

## Tooling dependencies
Rust crate dependencies (clap, roxmltree, …) are listed with their licenses in
the respective `Cargo.toml` files and resolved by Cargo; all are permissively
licensed (MIT/Apache-2.0).

[mermaid]: https://github.com/mermaid-js/mermaid
